use dartscope_core::pubspec::{PubspecConfiguration, PubspecConfigurationAnalysis};
use dartscope_core::{DartDiagnostic, PubspecAnalysis, PubspecInput};

use crate::pubspec_syntax::{
    PubspecSyntaxCheck, append_common_syntax_diagnostics, prepare_pubspec_source,
};
use crate::source_lines::source_lines;

pub(crate) fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let prepared = prepare_pubspec_source(&input.source);
    let marked_source = sanitize_bare_wildcards(&prepared.source, &prepared.syntax);
    let prepared_input = PubspecInput::new(input.path, marked_source);
    let mut analysis = crate::pubspec_yaml_marked_dependencies::parse_pubspec(
        prepared_input.clone(),
        &prepared.syntax,
    );
    let configuration =
        crate::pubspec_yaml_marked_configuration::parse_pubspec_configuration(prepared_input);
    merge_configuration(&mut analysis, configuration);

    analysis.diagnostics.retain(|diagnostic| {
        !matches!(
            diagnostic.code.as_str(),
            "pubspec_duplicate_key" | "pubspec_multiple_documents_unsupported"
        )
    });
    append_common_syntax_diagnostics(&mut analysis.diagnostics, &analysis.path, &prepared.syntax);
    apply_dependency_syntax_check(&mut analysis, &prepared.syntax);
    analysis
}

fn merge_configuration(
    analysis: &mut PubspecAnalysis,
    configuration: PubspecConfigurationAnalysis,
) {
    let PubspecConfigurationAnalysis {
        environment,
        flutter,
        diagnostics,
        ..
    } = configuration;
    analysis.configuration = PubspecConfiguration {
        environment,
        flutter,
    };
    for diagnostic in diagnostics {
        if !analysis.diagnostics.contains(&diagnostic) {
            analysis.diagnostics.push(diagnostic);
        }
    }
}

fn sanitize_bare_wildcards(source: &str, syntax: &PubspecSyntaxCheck) -> String {
    let mut bytes = source.as_bytes().to_vec();
    for line in source_lines(source) {
        if !syntax.is_bare_wildcard_line(line.number) {
            continue;
        }
        let Some(colon) = find_unquoted_colon(line.text) else {
            continue;
        };
        let Some(relative) = line.text[colon + 1..].find('*') else {
            continue;
        };
        bytes[line.byte_start + colon + 1 + relative] = b'0';
    }
    String::from_utf8(bytes).expect("single-byte wildcard replacement preserves UTF-8")
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

fn apply_dependency_syntax_check(analysis: &mut PubspecAnalysis, syntax: &PubspecSyntaxCheck) {
    analysis.diagnostics.retain(|diagnostic| {
        diagnostic.code != "pubspec_unsupported_yaml_alias"
            || !diagnostic
                .span
                .as_ref()
                .is_some_and(|span| syntax.is_bare_wildcard_line(span.start_line))
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
    fn preserves_bare_wildcard_as_dependency_version() {
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
    fn omits_alias_and_merge_dependency_values() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            concat!(
                "name: demo\n",
                "dependencies:\n",
                "  defaults: &defaults\n",
                "    path: ../defaults\n",
                "  aliased: *defaults\n",
                "  merged:\n",
                "    <<: *defaults\n",
            ),
        ));

        assert!(
            analysis
                .dependencies
                .iter()
                .any(|dependency| dependency.name == "defaults")
        );
        assert!(
            !analysis
                .dependencies
                .iter()
                .any(|dependency| dependency.name == "aliased")
        );
        assert!(
            !analysis
                .dependencies
                .iter()
                .any(|dependency| dependency.name == "merged")
        );
    }

    #[test]
    fn omits_malformed_inline_dependency_mapping() {
        let analysis = parse_pubspec(PubspecInput::new(
            "pubspec.yaml",
            concat!(
                "name: demo\n",
                "dependencies:\n",
                "  broken: { git: { url: https://example.com/repo.git }\n",
            ),
        ));

        assert!(analysis.dependencies.is_empty());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "pubspec_invalid_yaml")
        );
    }
}
