use dartscope_core::{DartDiagnostic, PubspecInput};

pub use dartscope_core::pubspec::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily,
};

use crate::pubspec_syntax::{append_common_syntax_diagnostics, prepare_pubspec_source};

const SUPPORTED_ASSET_PLATFORMS: [&str; 6] = ["android", "ios", "web", "linux", "macos", "windows"];

/// Parses environment constraints and normalized Flutter pubspec configuration.
pub fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    crate::pubspec_backend::parse_pubspec_configuration_with_backend(
        input,
        crate::pubspec_backend::DEFAULT_PUBSPEC_BACKEND,
    )
}

pub(crate) fn parse_pubspec_configuration_conservative(
    input: PubspecInput,
) -> PubspecConfigurationAnalysis {
    let prepared = prepare_pubspec_source(&input.source);
    let mut analysis =
        parse_pubspec_configuration_prepared(PubspecInput::new(input.path, prepared.source));
    append_common_syntax_diagnostics(&mut analysis.diagnostics, &analysis.path, &prepared.syntax);
    analysis
}

pub(crate) fn parse_pubspec_configuration_prepared(
    input: PubspecInput,
) -> PubspecConfigurationAnalysis {
    let source = input.source.clone();
    let mut analysis = crate::pubspec_configuration_legacy::parse_pubspec_configuration(input);
    let assets = crate::pubspec_assets::parse_flutter_assets(&source, &analysis.path);

    if assets.found_section {
        analysis.diagnostics.retain(|diagnostic| {
            !matches!(
                diagnostic.code.as_str(),
                "pubspec_unsupported_flutter_asset" | "pubspec_invalid_flutter_asset"
            )
        });
        analysis.flutter.assets = assets.assets;
        analysis.flutter.asset_configurations = assets.configurations;
        for diagnostic in assets.diagnostics {
            if !analysis.diagnostics.contains(&diagnostic) {
                analysis.diagnostics.push(diagnostic);
            }
        }
        validate_asset_selectors(&mut analysis);
    }

    analysis
}

fn validate_asset_selectors(analysis: &mut PubspecConfigurationAnalysis) {
    let mut diagnostics = Vec::new();
    for asset in &analysis.flutter.asset_configurations {
        for flavor in &asset.flavors {
            if flavor.is_empty() {
                diagnostics.push(
                    DartDiagnostic::error(
                        "pubspec_invalid_flutter_asset_flavor",
                        "Flutter asset flavor names cannot be empty",
                        Some(asset.span.clone()),
                    )
                    .with_path(analysis.path.clone()),
                );
            }
        }
        for platform in &asset.platforms {
            if !SUPPORTED_ASSET_PLATFORMS.contains(&platform.as_str()) {
                diagnostics.push(
                    DartDiagnostic::error(
                        "pubspec_invalid_flutter_asset_platform",
                        format!(
                            "unsupported Flutter asset platform: {platform}; expected one of {}",
                            SUPPORTED_ASSET_PLATFORMS.join(", ")
                        ),
                        Some(asset.span.clone()),
                    )
                    .with_path(analysis.path.clone()),
                );
            }
        }
    }
    for diagnostic in diagnostics {
        if !analysis.diagnostics.contains(&diagnostic) {
            analysis.diagnostics.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_complete_assets_with_legacy_paths() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "pubspec.yaml",
            concat!(
                "flutter:\n",
                "  assets:\n",
                "    - assets/plain.json\n",
                "    - path: assets/logo.svg\n",
                "      flavors: [development, production]\n",
                "      platforms: [android, ios]\n",
                "      transformers:\n",
                "        - package: vector_graphics_compiler\n",
                "          args: ['--tessellate', '--font-size=14']\n",
            ),
        ));

        assert!(analysis.diagnostics.is_empty());
        assert_eq!(analysis.flutter.assets.len(), 2);
        assert_eq!(analysis.flutter.asset_configurations.len(), 2);
        assert_eq!(
            analysis.flutter.asset_configurations[1].flavors,
            ["development", "production"]
        );
        assert_eq!(
            analysis.flutter.asset_configurations[1].platforms,
            ["android", "ios"]
        );
        assert_eq!(
            analysis.flutter.asset_configurations[1].transformers[0].package,
            "vector_graphics_compiler"
        );
    }

    #[test]
    fn accepts_custom_flavors_and_official_asset_platforms() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "pubspec.yaml",
            concat!(
                "flutter:\n",
                "  assets:\n",
                "    - path: assets/shared.bin\n",
                "      flavors: [customer-a, experimental_2026]\n",
                "      platforms: [android, ios, web, linux, macos, windows]\n",
            ),
        ));

        assert!(analysis.diagnostics.is_empty());
        assert_eq!(
            analysis.flutter.asset_configurations[0].flavors,
            ["customer-a", "experimental_2026"]
        );
    }

    #[test]
    fn diagnoses_empty_flavors_and_unknown_platforms() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "config\\pubspec.yaml",
            concat!(
                "flutter:\n",
                "  assets:\n",
                "    - path: assets/fuchsia.bin\n",
                "      flavors: ['']\n",
                "      platforms: [fuchsia]\n",
            ),
        ));

        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_asset_flavor"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_asset_platform"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
                && diagnostic.message.contains("fuchsia")
        }));
    }

    #[test]
    fn removes_legacy_warnings_for_supported_asset_metadata() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "pubspec.yaml",
            concat!(
                "flutter:\n",
                "  assets:\n",
                "    - path: assets/logo.svg\n",
                "      transformers:\n",
                "        - package: vector_graphics_compiler\n",
            ),
        ));

        assert!(
            !analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "pubspec_unsupported_flutter_asset")
        );
    }

    #[test]
    fn ignores_configuration_from_additional_documents() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "config\\pubspec.yaml",
            concat!(
                "---\n",
                "flutter:\n",
                "  generate: true\n",
                "---\n",
                "flutter:\n",
                "  generate: false\n",
            ),
        ));

        assert_eq!(analysis.flutter.generate_localizations, Some(true));
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_multiple_documents_unsupported"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
    }
}
