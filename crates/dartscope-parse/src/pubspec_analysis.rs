use dartscope_core::pubspec::{PubspecConfiguration, PubspecConfigurationAnalysis};
use dartscope_core::{PubspecAnalysis, PubspecInput};

/// Parses dependencies and typed configuration into the primary pubspec analysis model.
pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let configuration_analysis = crate::pubspec_configuration::parse_pubspec_configuration(input.clone());
    let mut analysis = crate::pubspec_dependencies::parse_pubspec(input);
    let PubspecConfigurationAnalysis {
        environment,
        flutter,
        diagnostics,
        ..
    } = configuration_analysis;

    analysis.configuration = PubspecConfiguration {
        environment,
        flutter,
    };
    for diagnostic in diagnostics {
        if !analysis.diagnostics.contains(&diagnostic) {
            analysis.diagnostics.push(diagnostic);
        }
    }
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_dependencies_environment_and_flutter_configuration() {
        let source = r#"name: demo
environment:
  sdk: ^3.4.0
dependencies:
  flutter:
    sdk: flutter
flutter:
  generate: true
  assets:
    - assets/
"#;

        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.dependencies.len(), 1);
        assert_eq!(analysis.configuration.environment.len(), 1);
        assert_eq!(analysis.configuration.environment[0].name, "sdk");
        assert_eq!(analysis.configuration.flutter.generate_localizations, Some(true));
        assert_eq!(analysis.configuration.flutter.assets.len(), 1);
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn deduplicates_shared_indentation_diagnostics() {
        let analysis = parse_pubspec(PubspecInput::new(
            "config\\pubspec.yaml",
            "name: demo\n\tbad: value\n",
        ));
        let count = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "pubspec_invalid_indentation")
            .count();

        assert_eq!(count, 1);
    }
}
