use dartscope_core::pubspec::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily,
};
use dartscope_core::{DartDiagnostic, PubspecInput, SourceSpan, normalize_path};

use crate::pubspec_yaml_marked::{Entry, Node, NodeKind, parse_marked_yaml};

const SUPPORTED_ASSET_PLATFORMS: [&str; 6] = ["android", "ios", "web", "linux", "macos", "windows"];

pub(crate) fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    let path = normalize_path(input.path);
    let document = parse_marked_yaml(&input.source);
    let mut analysis = PubspecConfigurationAnalysis {
        path: path.clone(),
        environment: Vec::new(),
        flutter: PubspecFlutterConfiguration::default(),
        diagnostics: document
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.with_path(path.clone()))
            .collect(),
    };

    let Some(root) = document.root.as_ref().and_then(Node::mapping) else {
        return analysis;
    };
    if let Some(environment) = last_entry(root, "environment") {
        parse_environment(environment, &path, &mut analysis);
    }
    if let Some(flutter) = last_entry(root, "flutter") {
        parse_flutter(flutter, &input.source, &path, &mut analysis);
    }
    analysis
}

impl Node {
    fn scalar_value(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Scalar(value) => Some(value),
            _ => None,
        }
    }

    fn mapping(&self) -> Option<&[Entry]> {
        match &self.kind {
            NodeKind::Mapping(entries) => Some(entries),
            _ => None,
        }
    }

    fn sequence(&self) -> Option<&[Node]> {
        match &self.kind {
            NodeKind::Sequence(items) => Some(items),
            _ => None,
        }
    }
}

fn last_entry<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Entry> {
    entries.iter().rev().find(|entry| entry.key == key)
}

fn parse_environment(environment: &Entry, path: &str, analysis: &mut PubspecConfigurationAnalysis) {
    let Some(entries) = environment.value.mapping() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_environment",
            "environment must be a mapping of scalar constraints",
            environment.value.span.clone(),
        );
        return;
    };
    for entry in entries {
        let Some(constraint) = entry.value.scalar_value() else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_environment",
                "environment constraints must have scalar values",
                entry.key_span.clone(),
            );
            continue;
        };
        analysis.environment.push(PubspecEnvironmentConstraint {
            name: entry.key.clone(),
            constraint: constraint.to_string(),
            span: entry.key_span.clone(),
        });
    }
}

fn parse_flutter(
    flutter: &Entry,
    source: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) {
    let Some(entries) = flutter.value.mapping() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_flutter_configuration",
            "flutter configuration must be a mapping",
            flutter.value.span.clone(),
        );
        return;
    };

    analysis.flutter.uses_material_design = parse_optional_bool(
        last_entry(entries, "uses-material-design"),
        "uses-material-design",
        path,
        analysis,
    );
    analysis.flutter.generate_localizations =
        parse_optional_bool(last_entry(entries, "generate"), "generate", path, analysis);
    if let Some(assets) = last_entry(entries, "assets") {
        parse_assets(assets, source, path, analysis);
    }
    if let Some(fonts) = last_entry(entries, "fonts") {
        parse_fonts(fonts, source, path, analysis);
    }
}

fn parse_optional_bool(
    entry: Option<&Entry>,
    key: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) -> Option<bool> {
    let entry = entry?;
    match entry.value.scalar_value() {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_boolean",
                format!("flutter.{key} must be true or false"),
                entry.key_span.clone(),
            );
            None
        }
    }
}

fn parse_assets(
    assets: &Entry,
    source: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) {
    let Some(items) = assets.value.sequence() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_flutter_asset",
            "Flutter assets must be a list",
            assets.key_span.clone(),
        );
        return;
    };
    for item in items {
        let configuration = match &item.kind {
            NodeKind::Scalar(value) if !value.is_empty() => PubspecFlutterAssetConfiguration {
                path: value.clone(),
                flavors: Vec::new(),
                platforms: Vec::new(),
                transformers: Vec::new(),
                span: source_line_span(source, &item.span),
            },
            NodeKind::Mapping(entries) => {
                let Some(path_entry) = last_entry(entries, "path") else {
                    push_error(
                        analysis,
                        path,
                        "pubspec_unsupported_flutter_asset",
                        "Flutter asset mappings must contain a scalar path field",
                        source_line_span(source, &item.span),
                    );
                    continue;
                };
                let Some(asset_path) = path_entry.value.scalar_value() else {
                    push_error(
                        analysis,
                        path,
                        "pubspec_invalid_flutter_asset",
                        "Flutter asset path must be a scalar",
                        source_line_span(source, &path_entry.key_span),
                    );
                    continue;
                };
                if asset_path.is_empty() {
                    push_error(
                        analysis,
                        path,
                        "pubspec_invalid_flutter_asset",
                        "Flutter asset path cannot be empty",
                        source_line_span(source, &path_entry.key_span),
                    );
                    continue;
                }
                PubspecFlutterAssetConfiguration {
                    path: asset_path.to_string(),
                    flavors: scalar_list(last_entry(entries, "flavors"), path, analysis),
                    platforms: scalar_list(last_entry(entries, "platforms"), path, analysis),
                    transformers: parse_transformers(
                        last_entry(entries, "transformers"),
                        source,
                        path,
                        analysis,
                    ),
                    span: source_line_span(source, &path_entry.key_span),
                }
            }
            _ => {
                push_error(
                    analysis,
                    path,
                    "pubspec_invalid_flutter_asset",
                    "Flutter asset entries must be scalar paths or mappings",
                    source_line_span(source, &item.span),
                );
                continue;
            }
        };
        validate_asset_selectors(&configuration, path, analysis);
        analysis.flutter.assets.push(PubspecFlutterAsset {
            path: configuration.path.clone(),
            span: configuration.span.clone(),
        });
        analysis.flutter.asset_configurations.push(configuration);
    }
}

fn scalar_list(
    entry: Option<&Entry>,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) -> Vec<String> {
    let Some(entry) = entry else {
        return Vec::new();
    };
    let Some(items) = entry.value.sequence() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_flutter_asset",
            "Flutter asset selectors must be a scalar list",
            entry.key_span.clone(),
        );
        return Vec::new();
    };
    let mut values = Vec::new();
    for item in items {
        if let Some(value) = item.scalar_value() {
            values.push(value.to_string());
        } else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset",
                "Flutter asset selectors must be scalar values",
                item.span.clone(),
            );
        }
    }
    values
}

fn parse_transformers(
    entry: Option<&Entry>,
    source: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) -> Vec<PubspecFlutterAssetTransformer> {
    let Some(entry) = entry else {
        return Vec::new();
    };
    let Some(items) = entry.value.sequence() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_flutter_asset_transformer",
            "Flutter asset transformers must be a list of mappings",
            entry.key_span.clone(),
        );
        return Vec::new();
    };
    let mut transformers = Vec::new();
    for item in items {
        let Some(entries) = item.mapping() else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer entries must contain a package",
                source_line_span(source, &item.span),
            );
            continue;
        };
        let Some(package_entry) = last_entry(entries, "package") else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer entries must contain a package",
                source_line_span(source, &item.span),
            );
            continue;
        };
        let Some(package) = package_entry
            .value
            .scalar_value()
            .filter(|value| !value.is_empty())
        else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer package cannot be empty",
                source_line_span(source, &package_entry.key_span),
            );
            continue;
        };
        let args = match last_entry(entries, "args") {
            Some(args) => scalar_sequence(
                args,
                path,
                analysis,
                "pubspec_invalid_flutter_asset_transformer",
                "Flutter asset transformer args must be a scalar list",
            ),
            None => Vec::new(),
        };
        transformers.push(PubspecFlutterAssetTransformer {
            package: package.to_string(),
            args,
            span: source_line_span(source, &package_entry.key_span),
        });
    }
    transformers
}

fn parse_fonts(
    fonts: &Entry,
    source: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) {
    let Some(families) = fonts.value.sequence() else {
        push_error(
            analysis,
            path,
            "pubspec_invalid_flutter_font",
            "Flutter fonts must be a list",
            fonts.key_span.clone(),
        );
        return;
    };
    for family_node in families {
        let Some(entries) = family_node.mapping() else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_font",
                "Flutter font list entries must contain a scalar mapping",
                source_line_span(source, &family_node.span),
            );
            continue;
        };
        let Some(family_entry) = last_entry(entries, "family") else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_font",
                "Flutter font list entries must contain a family",
                source_line_span(source, &family_node.span),
            );
            continue;
        };
        let Some(family) = family_entry.value.scalar_value() else {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_font",
                "Flutter font family must be a scalar",
                family_entry.key_span.clone(),
            );
            continue;
        };
        let mut parsed = PubspecFlutterFontFamily {
            family: family.to_string(),
            fonts: Vec::new(),
            span: source_line_span(source, &family_entry.key_span),
        };
        if let Some(fonts_entry) = last_entry(entries, "fonts") {
            if let Some(font_items) = fonts_entry.value.sequence() {
                for font_node in font_items {
                    if let Some(font) = parse_font(font_node, source, path, analysis) {
                        parsed.fonts.push(font);
                    }
                }
            } else {
                push_error(
                    analysis,
                    path,
                    "pubspec_invalid_flutter_font",
                    "Flutter family fonts must be a list",
                    fonts_entry.key_span.clone(),
                );
            }
        }
        analysis.flutter.fonts.push(parsed);
    }
}

fn parse_font(
    node: &Node,
    source: &str,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) -> Option<PubspecFlutterFont> {
    let entries = node.mapping()?;
    let asset_entry = last_entry(entries, "asset")?;
    let asset = asset_entry.value.scalar_value()?;
    let style = last_entry(entries, "style")
        .and_then(|entry| entry.value.scalar_value())
        .map(str::to_string);
    let weight = last_entry(entries, "weight").and_then(|entry| {
        let parsed = entry.value.scalar_value()?.parse::<u16>().ok();
        match parsed {
            Some(weight) if (100..=900).contains(&weight) && weight % 100 == 0 => Some(weight),
            _ => {
                push_error(
                    analysis,
                    path,
                    "pubspec_invalid_flutter_font_weight",
                    "Flutter font weight must be one of 100, 200, ..., 900",
                    entry.key_span.clone(),
                );
                None
            }
        }
    });
    Some(PubspecFlutterFont {
        asset: asset.to_string(),
        style,
        weight,
        span: source_line_span(source, &asset_entry.key_span),
    })
}

fn scalar_sequence(
    entry: &Entry,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
    code: &'static str,
    message: &'static str,
) -> Vec<String> {
    let Some(items) = entry.value.sequence() else {
        push_error(analysis, path, code, message, entry.key_span.clone());
        return Vec::new();
    };
    let mut values = Vec::new();
    for item in items {
        if let Some(value) = item.scalar_value() {
            values.push(value.to_string());
        } else {
            push_error(analysis, path, code, message, item.span.clone());
        }
    }
    values
}

fn validate_asset_selectors(
    asset: &PubspecFlutterAssetConfiguration,
    path: &str,
    analysis: &mut PubspecConfigurationAnalysis,
) {
    for flavor in &asset.flavors {
        if flavor.is_empty() {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset_flavor",
                "Flutter asset flavor names cannot be empty",
                asset.span.clone(),
            );
        }
    }
    for platform in &asset.platforms {
        if !SUPPORTED_ASSET_PLATFORMS.contains(&platform.as_str()) {
            push_error(
                analysis,
                path,
                "pubspec_invalid_flutter_asset_platform",
                format!(
                    "unsupported Flutter asset platform: {platform}; expected one of {}",
                    SUPPORTED_ASSET_PLATFORMS.join(", ")
                ),
                asset.span.clone(),
            );
        }
    }
}

fn source_line_span(source: &str, evidence: &SourceSpan) -> SourceSpan {
    let marker = evidence.byte_start.min(source.len());
    let byte_start = source[..marker].rfind('\n').map_or(0, |index| index + 1);
    let byte_end = source[marker..]
        .find('\n')
        .map_or(source.len(), |index| marker + index);
    let line = &source[byte_start..byte_end];
    SourceSpan::line(
        evidence.start_line,
        byte_start,
        line.strip_suffix('\r').unwrap_or(line),
    )
}

fn push_error(
    analysis: &mut PubspecConfigurationAnalysis,
    path: &str,
    code: &'static str,
    message: impl Into<String>,
    span: SourceSpan,
) {
    analysis
        .diagnostics
        .push(DartDiagnostic::error(code, message, Some(span)).with_path(path.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_environment_assets_transformers_and_fonts() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "config\\pubspec.yaml",
            concat!(
                "environment:\n",
                "  sdk: ^3.4.0\n",
                "flutter:\n",
                "  uses-material-design: true\n",
                "  generate: true\n",
                "  assets:\n",
                "    - path: assets/logo.svg\n",
                "      flavors: [development, customer-a]\n",
                "      platforms: [android, web]\n",
                "      transformers:\n",
                "        - package: vector_graphics_compiler\n",
                "          args: ['--tessellate']\n",
                "  fonts:\n",
                "    - family: Roboto\n",
                "      fonts:\n",
                "        - asset: fonts/Roboto-Regular.ttf\n",
                "          weight: 400\n",
            ),
        ));

        assert_eq!(analysis.path, "config/pubspec.yaml");
        assert_eq!(analysis.environment[0].name, "sdk");
        assert_eq!(analysis.environment[0].constraint, "^3.4.0");
        assert_eq!(analysis.flutter.uses_material_design, Some(true));
        assert_eq!(analysis.flutter.generate_localizations, Some(true));
        assert_eq!(analysis.flutter.asset_configurations.len(), 1);
        assert_eq!(
            analysis.flutter.asset_configurations[0].transformers[0].args,
            ["--tessellate"]
        );
        assert_eq!(analysis.flutter.fonts[0].fonts[0].weight, Some(400));
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn validates_asset_selectors_with_asset_line_evidence() {
        let source = concat!(
            "flutter:\n",
            "  assets:\n",
            "    - path: assets/fuchsia.bin\n",
            "      flavors: ['']\n",
            "      platforms: [fuchsia]\n",
        );
        let expected = source.find("    - path").expect("asset line");
        let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.diagnostics.len(), 2);
        assert!(analysis.diagnostics.iter().all(|diagnostic| {
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| span.byte_start == expected)
        }));
    }

    #[test]
    fn preserves_crlf_and_unicode_asset_byte_offsets() {
        let source = concat!(
            "---\r\n",
            "description: Привет\r\n",
            "flutter:\r\n",
            "  assets:\r\n",
            "    - path: assets/иконка.png\r\n",
            "...\r\n",
        );
        let expected = source.find("    - path").expect("asset line");
        let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(
            analysis.flutter.asset_configurations[0].span.byte_start,
            expected
        );
        assert!(analysis.diagnostics.is_empty());
    }
}
