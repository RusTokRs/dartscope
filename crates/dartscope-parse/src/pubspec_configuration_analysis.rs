use dartscope_core::PubspecInput;

pub use dartscope_core::pubspec::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily,
};

/// Parses environment constraints and normalized Flutter pubspec configuration.
pub fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    crate::pubspec_backend::parse_pubspec_configuration(input)
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
