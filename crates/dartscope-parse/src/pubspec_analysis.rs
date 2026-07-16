use dartscope_core::{PubspecAnalysis, PubspecInput};

/// Parses dependencies and typed configuration into the primary pubspec analysis model.
pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    crate::pubspec_backend::parse_pubspec(input)
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
        assert_eq!(
            analysis.configuration.flutter.generate_localizations,
            Some(true)
        );
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
        assert!(
            !analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "pubspec_unsupported_yaml_alias")
        );
    }

    #[test]
    fn preserves_named_alias_diagnostics() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            "name: demo\ndependencies:\n  aliased: *defaults\n",
        ));

        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "pubspec_unsupported_yaml_alias")
        );
    }

    #[test]
    fn accepts_single_explicit_document_markers() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            "---\r\nname: демо\r\ndependencies:\r\n  flutter:\r\n    sdk: flutter\r\n...\r\n",
        ));

        assert_eq!(analysis.package_name.as_deref(), Some("демо"));
        assert_eq!(analysis.dependencies.len(), 1);
        assert!(
            !analysis
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "pubspec_multiple_documents_unsupported" })
        );
    }

    #[test]
    fn ignores_additional_documents() {
        let analysis = parse_pubspec(PubspecInput::new(
            "config\\pubspec.yaml",
            concat!(
                "name: first\n",
                "dependencies:\n",
                "  first: ^1.0.0\n",
                "---\n",
                "name: second\n",
                "dependencies:\n",
                "  second: ^2.0.0\n",
            ),
        ));

        assert_eq!(analysis.package_name.as_deref(), Some("first"));
        assert_eq!(analysis.dependencies.len(), 1);
        assert_eq!(analysis.dependencies[0].name, "first");
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_multiple_documents_unsupported"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
                && diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| span.start_line == 4)
        }));
    }

    #[test]
    fn diagnoses_duplicate_top_level_and_direct_mapping_keys() {
        let source = concat!(
            "name: first\r\n",
            "name: второй\r\n",
            "dependencies:\r\n",
            "  shared: ^1.0.0\r\n",
            "  shared: ^2.0.0\r\n",
            "environment:\r\n",
            "  sdk: ^3.4.0\r\n",
            "  sdk: ^3.5.0\r\n",
            "flutter:\r\n",
            "  generate: true\r\n",
            "  generate: false\r\n",
        );
        let analysis = parse_pubspec(PubspecInput::new("config\\pubspec.yaml", source));
        let duplicates = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "pubspec_duplicate_key")
            .collect::<Vec<_>>();

        assert_eq!(duplicates.len(), 4);
        assert!(
            duplicates
                .iter()
                .all(|diagnostic| { diagnostic.path.as_deref() == Some("config/pubspec.yaml") })
        );
        let sdk = duplicates
            .iter()
            .find(|diagnostic| {
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| span.start_line == 8)
            })
            .expect("duplicate environment key diagnostic");
        let span = sdk.span.as_ref().expect("duplicate key span");
        let expected_start = source.find("  sdk: ^3.5.0").expect("second sdk") + 2;
        assert_eq!(span.byte_start, expected_start);
        assert_eq!(span.byte_end, expected_start + "sdk".len());
        assert_eq!(span.start_column, 3);
        assert_eq!(span.end_column, 6);
    }

    #[test]
    fn rejects_unbalanced_dependency_flow_syntax() {
        for dependency in [
            "broken: { path: ../local } }",
            "broken: { git: { url: https://example.com/repo.git ] }",
            "broken: { path: \"unterminated }",
        ] {
            let source = format!("name: demo\ndependencies:\n  {dependency}\n");
            let analysis = parse_pubspec(PubspecInput::new("config\\pubspec.yaml", source));

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
