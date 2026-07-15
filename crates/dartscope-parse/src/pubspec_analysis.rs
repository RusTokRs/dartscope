use dartscope_core::pubspec::{PubspecConfiguration, PubspecConfigurationAnalysis};
use dartscope_core::{DartDiagnostic, PubspecAnalysis, PubspecInput};

use crate::pubspec_syntax::{check_pubspec_syntax, PubspecSyntaxCheck};

/// Parses dependencies and typed configuration into the primary pubspec analysis model.
pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let syntax = check_pubspec_syntax(&input.source);
    let configuration_analysis =
        crate::pubspec_configuration::parse_pubspec_configuration(input.clone());
    let mut analysis = crate::pubspec_dependencies::parse_pubspec(input);
    apply_dependency_syntax_check(&mut analysis, &syntax);

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

fn apply_dependency_syntax_check(
    analysis: &mut PubspecAnalysis,
    syntax: &PubspecSyntaxCheck,
) {
    analysis.diagnostics.retain(|diagnostic| {
        diagnostic.code != "pubspec_unsupported_yaml_alias"
            || !diagnostic.span.as_ref().is_some_and(|span| {
                syntax.is_bare_wildcard_line(span.start_line)
            })
    });

    for span in syntax.invalid_flow_spans() {
        analysis
            .dependencies
            .retain(|dependency| dependency.span.start_line != span.start_line);
        let already_reported = analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_yaml"
                && diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|existing| existing.start_line == span.start_line)
        });
        if !already_reported {
            analysis.diagnostics.push(
                DartDiagnostic::error(
                    "pubspec_invalid_yaml",
                    "invalid inline dependency source mapping",
                    Some(span.clone()),
                )
                .with_path(analysis.path.clone()),
            );
        }
    }
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

    #[test]
    fn preserves_bare_wildcard_without_alias_diagnostic() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            "name: demo\ndependencies:\n  any_version: *\n",
        ));

        assert_eq!(analysis.dependencies.len(), 1);
        assert_eq!(
            analysis.dependencies[0].version_or_source.as_deref(),
            Some("*")
        );
        assert!(!analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "pubspec_unsupported_yaml_alias"));
    }

    #[test]
    fn preserves_named_alias_diagnostics() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            "name: demo\ndependencies:\n  aliased: *defaults\n",
        ));

        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "pubspec_unsupported_yaml_alias"));
    }

    #[test]
    fn rejects_unbalanced_dependency_flow_syntax() {
        for dependency in [
            "broken: { path: ../local } }",
            "broken: { git: { url: https://example.com/repo.git ] }",
            "broken: { path: \"unterminated }",
        ] {
            let source = format!("name: demo\ndependencies:\n  {dependency}\n");
            let analysis = parse_pubspec(PubspecInput::new(
                "config\\pubspec.yaml",
                source,
            ));

            assert!(analysis.dependencies.is_empty(), "{dependency}");
            assert!(analysis.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "pubspec_invalid_yaml"
                    && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
            }));
        }
    }

    #[test]
    fn preserves_quoted_commas_in_dependency_flow_syntax() {
        let source = concat!(
            "name: demo\n",
            "dependencies:\n",
            "  remote: { git: { url: \"https://example.com/repo.git?parts=one,two\", ref: stable } }\n",
        );
        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.dependencies.len(), 1);
        assert_eq!(
            analysis.dependencies[0].version_or_source.as_deref(),
            Some("git:ref=stable;url=https://example.com/repo.git?parts=one,two")
        );
        assert!(analysis.diagnostics.is_empty());
    }
}
