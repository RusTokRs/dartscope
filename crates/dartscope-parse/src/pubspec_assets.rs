use dartscope_core::pubspec::{
    PubspecFlutterAsset, PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer,
};
use dartscope_core::{DartDiagnostic, SourceSpan};

use crate::pubspec_yaml_subset::{
    leading_indentation_contains_tab, leading_space_count, parse_inline_sequence,
    set_or_matches_indent, strip_yaml_comment, yaml_key_value, yaml_scalar,
};
use crate::source_lines::{SourceLine, source_lines};

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
        asset_property_indent: None,
        selector_item_indent: None,
        mode: AssetMode::None,
        transformer_indent: None,
        transformer_property_indent: None,
        args_item_indent: None,
        current_asset_is_mapping: false,
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
    asset_property_indent: Option<usize>,
    selector_item_indent: Option<usize>,
    mode: AssetMode,
    transformer_indent: Option<usize>,
    transformer_property_indent: Option<usize>,
    args_item_indent: Option<usize>,
    current_asset_is_mapping: bool,
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
            self.reset_asset_section();
            return;
        }
        if !self.in_flutter {
            return;
        }

        let direct_indent = *self.flutter_direct_indent.get_or_insert(indent);
        if indent < direct_indent {
            self.finish_asset();
            self.in_flutter = false;
            self.reset_asset_section();
            return;
        }
        if indent == direct_indent {
            self.finish_asset();
            self.reset_asset_section();
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
        self.reset_asset_item();

        let Some(item) = trimmed.strip_prefix('-').map(str::trim) else {
            self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                "Flutter asset entries must be list items",
                Some(span),
            ));
            return;
        };
        let (path, is_mapping) = match yaml_key_value(item) {
            Some(("path", Some(path))) => (yaml_scalar(path), true),
            Some(_) => {
                self.push_diagnostic(DartDiagnostic::warning(
                    "pubspec_unsupported_flutter_asset",
                    "Flutter asset mappings must start with a scalar path field",
                    Some(span),
                ));
                return;
            }
            None => (yaml_scalar(item), false),
        };
        if path.is_empty() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset path cannot be empty",
                Some(span),
            ));
            return;
        }

        self.current_asset_is_mapping = is_mapping;
        self.current_asset = Some(PubspecFlutterAssetConfiguration {
            path: path.to_string(),
            flavors: Vec::new(),
            platforms: Vec::new(),
            transformers: Vec::new(),
            span,
        });
    }

    fn observe_asset_property(&mut self, trimmed: &str, indent: usize, span: SourceSpan) {
        if !self.current_asset_is_mapping {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset metadata requires a mapping with a path field",
                Some(span),
            ));
            return;
        }
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
        if self.current_transformer.is_some()
            && self
                .transformer_indent
                .is_some_and(|transformer_indent| indent >= transformer_indent)
        {
            self.observe_transformer_property(key, value, indent, span);
            return;
        }

        self.finish_transformer();
        self.reset_transformer_state();
        self.mode = AssetMode::None;
        self.selector_item_indent = None;
        let parent_indent = self.asset_item_indent.unwrap_or_default();
        if !set_or_matches_indent(&mut self.asset_property_indent, indent, parent_indent) {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset mapping fields must use consistent indentation",
                Some(span),
            ));
            return;
        }

        match (key, value) {
            ("flavors", value) => self.set_string_list(AssetMode::Flavors, value, span),
            ("platforms", value) => self.set_string_list(AssetMode::Platforms, value, span),
            ("transformers", None) => {
                self.mode = AssetMode::Transformers;
                self.transformer_indent = None;
            }
            ("transformers", Some(value))
                if parse_inline_sequence(value).is_some_and(|values| values.is_empty()) =>
            {
                self.mode = AssetMode::None;
            }
            ("transformers", Some(_)) => self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformers must be a block list of mappings",
                Some(span),
            )),
            _ => self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                format!("unsupported Flutter asset field: {key}"),
                Some(span),
            )),
        }
    }

    fn observe_transformer_property(
        &mut self,
        key: &str,
        value: Option<&str>,
        indent: usize,
        span: SourceSpan,
    ) {
        let parent_indent = self.transformer_indent.unwrap_or_default();
        if !set_or_matches_indent(&mut self.transformer_property_indent, indent, parent_indent) {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer fields must use consistent indentation",
                Some(span),
            ));
            return;
        }

        self.mode = AssetMode::Transformers;
        self.args_item_indent = None;
        match (key, value) {
            ("args", None) => {
                self.mode = AssetMode::TransformerArgs;
                self.args_item_indent = None;
            }
            ("args", Some(value)) => self.set_transformer_args(value, span),
            _ => self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset_transformer",
                format!("unsupported Flutter asset transformer field: {key}"),
                Some(span),
            )),
        }
    }

    fn set_string_list(&mut self, mode: AssetMode, value: Option<&str>, span: SourceSpan) {
        self.selector_item_indent = None;
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
            self.mode = AssetMode::None;
            return;
        };
        if let Some(asset) = self.current_asset.as_mut() {
            match mode {
                AssetMode::Flavors => asset.flavors.extend(values),
                AssetMode::Platforms => asset.platforms.extend(values),
                _ => {}
            }
        }
        self.mode = AssetMode::None;
    }

    fn set_transformer_args(&mut self, value: &str, span: SourceSpan) {
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
        self.mode = AssetMode::Transformers;
        self.args_item_indent = None;
    }

    fn observe_nested_item(&mut self, item: &str, indent: usize, span: SourceSpan) {
        if self.mode == AssetMode::TransformerArgs
            && self
                .transformer_indent
                .is_some_and(|transformer_indent| indent == transformer_indent)
        {
            self.mode = AssetMode::Transformers;
        }

        match self.mode {
            AssetMode::Flavors | AssetMode::Platforms => {
                self.push_selector(item, indent, self.mode, span);
            }
            AssetMode::Transformers => self.start_transformer(item, indent, span),
            AssetMode::TransformerArgs => self.push_transformer_arg(item, indent, span),
            AssetMode::None => self.push_diagnostic(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                "unexpected nested Flutter asset list item",
                Some(span),
            )),
        }
    }

    fn push_selector(&mut self, item: &str, indent: usize, mode: AssetMode, span: SourceSpan) {
        let parent_indent = self.asset_property_indent.unwrap_or_default();
        if !set_or_matches_indent(&mut self.selector_item_indent, indent, parent_indent) {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset selector items must use consistent indentation",
                Some(span),
            ));
            return;
        }
        let item = item.trim();
        if item.is_empty() || yaml_key_value(item).is_some() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset selectors must be scalar values",
                Some(span),
            ));
            return;
        }
        let value = yaml_scalar(item);
        if let Some(asset) = self.current_asset.as_mut() {
            match mode {
                AssetMode::Flavors => asset.flavors.push(value.to_string()),
                AssetMode::Platforms => asset.platforms.push(value.to_string()),
                _ => {}
            }
        }
    }

    fn start_transformer(&mut self, item: &str, indent: usize, span: SourceSpan) {
        let parent_indent = self.asset_property_indent.unwrap_or_default();
        if !set_or_matches_indent(&mut self.transformer_indent, indent, parent_indent) {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer items must use consistent indentation",
                Some(span),
            ));
            return;
        }

        self.finish_transformer();
        self.transformer_property_indent = None;
        self.args_item_indent = None;

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
        self.current_transformer = Some(PubspecFlutterAssetTransformer {
            package: package.to_string(),
            args: Vec::new(),
            span,
        });
    }

    fn push_transformer_arg(&mut self, item: &str, indent: usize, span: SourceSpan) {
        let parent_indent = self.transformer_property_indent.unwrap_or_default();
        if !set_or_matches_indent(&mut self.args_item_indent, indent, parent_indent) {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer args must use consistent indentation",
                Some(span),
            ));
            return;
        }
        let item = item.trim();
        if item.is_empty() || yaml_key_value(item).is_some() {
            self.push_diagnostic(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer args must be scalar values",
                Some(span),
            ));
            return;
        }
        if let Some(transformer) = self.current_transformer.as_mut() {
            transformer.args.push(yaml_scalar(item).to_string());
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
        self.reset_asset_item();
    }

    fn reset_asset_section(&mut self) {
        self.in_assets = false;
        self.asset_item_indent = None;
        self.reset_asset_item();
    }

    fn reset_asset_item(&mut self) {
        self.asset_property_indent = None;
        self.selector_item_indent = None;
        self.mode = AssetMode::None;
        self.current_asset_is_mapping = false;
        self.reset_transformer_state();
    }

    fn reset_transformer_state(&mut self) {
        self.transformer_indent = None;
        self.transformer_property_indent = None;
        self.args_item_indent = None;
        self.current_transformer = None;
    }

    fn push_diagnostic(&mut self, diagnostic: DartDiagnostic) {
        self.diagnostics
            .push(diagnostic.with_path(self.path.clone()));
    }
}
