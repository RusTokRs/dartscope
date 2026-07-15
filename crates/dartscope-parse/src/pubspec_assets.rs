use dartscope_core::pubspec::{
    PubspecFlutterAsset, PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer,
};
use dartscope_core::{DartDiagnostic, SourceSpan};

use crate::source_lines::{source_lines, SourceLine};

pub(crate) struct PubspecAssetParse {
    pub(crate) found_section: bool,
    pub(crate) assets: Vec<PubspecFlutterAsset>,
    pub(crate) configurations: Vec<PubspecFlutterAssetConfiguration>,
    pub(crate) diagnostics: Vec<DartDiagnostic>,
}

pub(crate) fn parse_flutter_assets(source: &str, path: &str) -> PubspecAssetParse {
    let mut parser = AssetParser {
        path: path.to_string(),
        found_section: false,
        assets: Vec::new(),
        configurations: Vec::new(),
        diagnostics: Vec::new(),
        in_flutter: false,
        flutter_direct_indent: None,
        in_assets: false,
        asset_item_indent: None,
        mode: AssetMode::None,
        transformer_indent: None,
        current_asset: None,
        current_transformer: None,
    };

    for line in source_lines(source) {
        parser.observe(line);
    }
    parser.finish_asset();

    PubspecAssetParse {
        found_section: parser.found_section,
        assets: parser.assets,
        configurations: parser.configurations,
        diagnostics: parser.diagnostics,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AssetMode {
    None,
    Flavors,
    Platforms,
    Transformers,
    TransformerArgs,
}

struct AssetParser {
    path: String,
    found_section: bool,
    assets: Vec<PubspecFlutterAsset>,
    configurations: Vec<PubspecFlutterAssetConfiguration>,
    diagnostics: Vec<DartDiagnostic>,
    in_flutter: bool,
    flutter_direct_indent: Option<usize>,
    in_assets: bool,
    asset_item_indent: Option<usize>,
    mode: AssetMode,
    transformer_indent: Option<usize>,
    current_asset: Option<PubspecFlutterAssetConfiguration>,
    current_transformer: Option<PubspecFlutterAssetTransformer>,
}

impl AssetParser {
    fn observe(&mut self, source_line: SourceLine<'_>) {
        if leading_indentation_contains_tab(source_line.text) {
            return;
        }
        let yaml = strip_yaml_comment(source_line.text);
        let trimmed = yaml.trim();
        if trimmed.is_empty() {
            return;
        }

        let indent = leading_space_count(source_line.text);
        let span = SourceSpan::line(source_line.number, source_line.byte_start, source_line.text);
        if indent == 0 {
            self.finish_asset();
            self.in_flutter = matches!(yaml_key_value(trimmed), Some(("flutter", None)));
            self.flutter_direct_indent = None;
            self.in_assets = false;
            self.asset_item_indent = None;
            self.mode = AssetMode::None;
            self.transformer_indent = None;
            return;
        }
        if !self.in_flutter {
            return;
        }

        let direct_indent = *self.flutter_direct_indent.get_or_insert(indent);
        if indent < direct_indent {
            self.finish_asset();
            self.in_flutter = false;
            self.in_assets = false;
            return;
        }
        if indent == direct_indent {
            self.finish_asset();
            self.asset_item_indent = None;
            self.mode = AssetMode::None;
            self.transformer_indent = None;
            self.in_assets = matches!(yaml_key_value(trimmed), Some(("assets", None)));
            self.found_section |= self.in_assets;
            return;
        }
        if !self.in_assets {
            return;
        }

        let item_indent = *self.asset_item_indent.get_or_insert(indent);
        if indent == item_indent {
            self.start_asset(trimmed, span);
        } else if indent > item_indent {
            self.observe_asset_property(trimmed, indent, span);
        } else {
            self.finish_asset();
            self.in_assets = false;
        }
    }

    fn start_asset(&mut self, trimmed: &str, span: SourceSpan) {
        self.finish_asset();
        self.mode = AssetMode::None;
        self.transformer_indent = None;

        let Some(item) = trimmed.strip_prefix('-').map(str::trim) else {
            self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                "Flutter asset entries must be list items",
                Some(span),
            ));
            return;
        };
        let path = match yaml_key_value(item) {
            Some(("path", Some(path))) => yaml_scalar(path),
            Some(_) => {
                self.push_diagnostic(DartDiagnostic::warning(
                    "pubspec_unsupported_flutter_asset",
                    "Flutter asset mappings must start with a scalar path field",
                    Some(span),
                ));
                return;
            }
            None => yaml_scalar(item),
        };
        if path.is_empty() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset path cannot be empty",
                Some(span),
            ));
            return;
        }

        self.current_asset = Some(PubspecFlutterAssetConfiguration {
            path: path.to_string(),
            flavors: Vec::new(),
            platforms: Vec::new(),
            transformers: Vec::new(),
            span,
        });
    }

    fn observe_asset_property(&mut self, trimmed: &str, indent: usize, span: SourceSpan) {
        if let Some(item) = trimmed.strip_prefix('-').map(str::trim) {
            self.observe_nested_item(item, indent, span);
            return;
        }

        let Some((key, value)) = yaml_key_value(trimmed) else {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "expected a Flutter asset mapping entry",
                Some(span),
            ));
            return;
        };
        match (key, value) {
            ("flavors", value) => self.set_string_list(AssetMode::Flavors, value, span),
            ("platforms", value) => self.set_string_list(AssetMode::Platforms, value, span),
            ("transformers", None) => {
                self.finish_transformer();
                self.mode = AssetMode::Transformers;
                self.transformer_indent = None;
            }
            ("transformers", Some(_)) => self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformers must be a list",
                Some(span),
            )),
            ("args", value) if self.current_transformer.is_some() => {
                self.set_transformer_args(value, span)
            }
            _ => self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                format!("unsupported Flutter asset field: {key}"),
                Some(span),
            )),
        }
    }

    fn set_string_list(&mut self, mode: AssetMode, value: Option<&str>, span: SourceSpan) {
        if self.current_asset.is_none() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset metadata appears before an asset path",
                Some(span),
            ));
            return;
        }
        self.finish_transformer();
        self.transformer_indent = None;
        self.mode = mode;
        let Some(value) = value else {
            return;
        };
        let Some(values) = parse_inline_sequence(value) else {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset selectors must be a scalar list",
                Some(span),
            ));
            return;
        };
        if let Some(asset) = self.current_asset.as_mut() {
            match mode {
                AssetMode::Flavors => asset.flavors.extend(values),
                AssetMode::Platforms => asset.platforms.extend(values),
                _ => {}
            }
        }
    }

    fn set_transformer_args(&mut self, value: Option<&str>, span: SourceSpan) {
        self.mode = AssetMode::TransformerArgs;
        let Some(value) = value else {
            return;
        };
        let Some(args) = parse_inline_sequence(value) else {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer args must be a scalar list",
                Some(span),
            ));
            return;
        };
        if let Some(transformer) = self.current_transformer.as_mut() {
            transformer.args.extend(args);
        }
    }

    fn observe_nested_item(&mut self, item: &str, indent: usize, span: SourceSpan) {
        if self.mode == AssetMode::TransformerArgs
            && self.transformer_indent.is_some_and(|expected| indent == expected)
        {
            self.mode = AssetMode::Transformers;
        }

        match self.mode {
            AssetMode::Flavors => self.push_selector(item, true, span),
            AssetMode::Platforms => self.push_selector(item, false, span),
            AssetMode::Transformers => self.start_transformer(item, indent, span),
            AssetMode::TransformerArgs => self.push_transformer_arg(item, span),
            AssetMode::None => self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                "unexpected nested Flutter asset list item",
                Some(span),
            )),
        }
    }

    fn push_selector(&mut self, item: &str, flavor: bool, span: SourceSpan) {
        let value = yaml_scalar(item);
        if value.is_empty() || yaml_key_value(value).is_some() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset selectors must be scalar values",
                Some(span),
            ));
            return;
        }
        if let Some(asset) = self.current_asset.as_mut() {
            if flavor {
                asset.flavors.push(value.to_string());
            } else {
                asset.platforms.push(value.to_string());
            }
        }
    }

    fn start_transformer(&mut self, item: &str, indent: usize, span: SourceSpan) {
        self.finish_transformer();
        let Some(("package", Some(package))) = yaml_key_value(item) else {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer entries must contain a package",
                Some(span),
            ));
            return;
        };
        let package = yaml_scalar(package);
        if package.is_empty() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer package cannot be empty",
                Some(span),
            ));
            return;
        }
        self.transformer_indent = Some(indent);
        self.current_transformer = Some(PubspecFlutterAssetTransformer {
            package: package.to_string(),
            args: Vec::new(),
            span,
        });
    }

    fn push_transformer_arg(&mut self, item: &str, span: SourceSpan) {
        let arg = yaml_scalar(item);
        if arg.is_empty() || yaml_key_value(arg).is_some() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer args must be scalar values",
                Some(span),
            ));
            return;
        }
        if let Some(transformer) = self.current_transformer.as_mut() {
            transformer.args.push(arg.to_string());
        }
    }

    fn finish_transformer(&mut self) {
        let Some(transformer) = self.current_transformer.take() else {
            return;
        };
        if let Some(asset) = self.current_asset.as_mut() {
            asset.transformers.push(transformer);
        }
    }

    fn finish_asset(&mut self) {
        self.finish_transformer();
        let Some(configuration) = self.current_asset.take() else {
            return;
        };
        self.assets.push(PubspecFlutterAsset {
            path: configuration.path.clone(),
            span: configuration.span.clone(),
        });
        self.configurations.push(configuration);
        self.mode = AssetMode::None;
        self.transformer_indent = None;
    }

    fn push_diagnostic(&mut self, diagnostic: DartDiagnostic) {
        self.diagnostics
            .push(diagnostic.with_path(self.path.clone()));
    }
}

fn parse_inline_sequence(value: &str) -> Option<Vec<String>> {
    let value = value.trim();
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut values = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut chars = inner.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ',' => {
                push_inline_scalar(&mut values, &inner[start..index])?;
                start = index + ch.len_utf8();
            }
            '[' | ']' | '{' | '}' => return None,
            _ => {}
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    push_inline_scalar(&mut values, &inner[start..])?;
    Some(values)
}

fn push_inline_scalar(values: &mut Vec<String>, value: &str) -> Option<()> {
    let value = yaml_scalar(value);
    if value.is_empty() || yaml_key_value(value).is_some() {
        return None;
    }
    values.push(value.to_string());
    Some(())
}

fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let colon = find_unquoted_colon(trimmed)?;
    let key = trimmed[..colon].trim();
    if key.is_empty() {
        return None;
    }
    let value = trimmed[colon + 1..].trim();
    Some((yaml_scalar(key), (!value.is_empty()).then_some(value)))
}

fn find_unquoted_colon(value: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                ':' => return Some(index),
                _ => {}
            }
        }
    }
    None
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;
    for (index, ch) in line.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '#' if previous.is_none_or(char::is_whitespace) => return &line[..index],
                _ => {}
            }
        }
        previous = Some(ch);
    }
    line
}

fn yaml_scalar(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if matches!((first, last), (b'\'', b'\'') | (b'"', b'"')) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn leading_indentation_contains_tab(line: &str) -> bool {
    line.chars()
        .take_while(|ch| ch.is_whitespace())
        .any(|ch| ch == '\t')
}

fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selectors_and_ordered_transformers() {
        let source = concat!(
            "flutter:\n",
            "  assets:\n",
            "    - path: assets/logo.svg\n",
            "      flavors: [development, production]\n",
            "      platforms:\n",
            "        - android\n",
            "        - ios\n",
            "      transformers:\n",
            "        - package: vector_graphics_compiler\n",
            "          args: ['--tessellate', '--font-size=14']\n",
            "        - package: png_optimizer\n",
        );
        let parsed = parse_flutter_assets(source, "pubspec.yaml");

        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.assets.len(), 1);
        let asset = &parsed.configurations[0];
        assert_eq!(asset.flavors, ["development", "production"]);
        assert_eq!(asset.platforms, ["android", "ios"]);
        assert_eq!(asset.transformers.len(), 2);
        assert_eq!(asset.transformers[0].package, "vector_graphics_compiler");
        assert_eq!(asset.transformers[0].args, ["--tessellate", "--font-size=14"]);
        assert_eq!(asset.transformers[1].package, "png_optimizer");
    }

    #[test]
    fn preserves_scalar_assets_in_both_representations() {
        let parsed = parse_flutter_assets(
            "flutter:\n  assets:\n    - assets/images/\n",
            "pubspec.yaml",
        );

        assert_eq!(parsed.assets[0].path, "assets/images/");
        assert_eq!(parsed.configurations[0].path, "assets/images/");
        assert!(parsed.configurations[0].transformers.is_empty());
    }

    #[test]
    fn diagnoses_transformers_without_packages() {
        let parsed = parse_flutter_assets(
            "flutter:\n  assets:\n    - path: assets/logo.svg\n      transformers:\n        - args: [bad]\n",
            "config/pubspec.yaml",
        );

        assert!(parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_asset_transformer"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
    }
}
