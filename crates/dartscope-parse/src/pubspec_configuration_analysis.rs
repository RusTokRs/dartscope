use dartscope_core::PubspecInput;

pub use dartscope_core::pubspec::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer,
    PubspecFlutterConfiguration, PubspecFlutterFont, PubspecFlutterFontFamily,
};

use crate::pubspec_syntax::{append_common_syntax_diagnostics, prepare_pubspec_source};

/// Parses environment constraints and normalized Flutter pubspec configuration.
pub fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    let prepared = prepare_pubspec_source(&input.source);
    let mut analysis = parse_pubspec_configuration_prepared(PubspecInput::new(
        input.path,
        prepared.source,
    ));
    append_common_syntax_diagnostics(
        &mut analysis.diagnostics,
        &analysis.path,
        &prepared.syntax,
    );
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
    }

    analysis
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

        assert!(!analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "pubspec_unsupported_flutter_asset"));
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
