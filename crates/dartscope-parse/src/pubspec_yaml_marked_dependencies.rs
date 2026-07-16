use std::collections::BTreeMap;

use dartscope_core::{
    normalize_path, DartDiagnostic, PubspecAnalysis, PubspecDependency, PubspecDependencySection,
    PubspecInput,
};

use crate::pubspec_source::parse_normalized_dependency_source;
use crate::pubspec_syntax::PubspecSyntaxCheck;
use crate::pubspec_yaml_marked::{parse_marked_yaml, Entry, Node, NodeKind};

pub(crate) fn parse_pubspec(
    input: PubspecInput,
    syntax: &PubspecSyntaxCheck,
) -> PubspecAnalysis {
    let path = normalize_path(input.path);
    let document = parse_marked_yaml(&input.source);
    let mut analysis = PubspecAnalysis {
        path: path.clone(),
        package_name: None,
        dependencies: Vec::new(),
        diagnostics: document
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.with_path(path.clone()))
            .collect(),
        ..PubspecAnalysis::default()
    };

    if let Some(entries) = document.root.as_ref().and_then(mapping) {
        parse_root(entries, syntax, &path, &mut analysis);
    }
    if analysis.package_name.is_none() {
        analysis.diagnostics.push(
            DartDiagnostic::warning(
                "pubspec_missing_name",
                "pubspec.yaml does not declare a package name",
                None,
            )
            .with_path(path),
        );
    }
    analysis
}

fn parse_root(
    entries: &[Entry],
    syntax: &PubspecSyntaxCheck,
    path: &str,
    analysis: &mut PubspecAnalysis,
) {
    let mut name = None;
    for entry in entries {
        match entry.key.as_str() {
            "name" => name = Some(entry),
            "dependencies" => parse_dependency_section(
                entry,
                PubspecDependencySection::Dependencies,
                syntax,
                path,
                analysis,
            ),
            "dev_dependencies" => parse_dependency_section(
                entry,
                PubspecDependencySection::DevDependencies,
                syntax,
                path,
                analysis,
            ),
            "dependency_overrides" => parse_dependency_section(
                entry,
                PubspecDependencySection::DependencyOverrides,
                syntax,
                path,
                analysis,
            ),
            _ => {}
        }
    }

    if let Some(entry) = name {
        if let Some(value) = scalar_value(&entry.value) {
            analysis.package_name = Some(value.to_string());
        } else {
            analysis.diagnostics.push(
                DartDiagnostic::error(
                    "pubspec_invalid_name",
                    "the pubspec package name must be a scalar value",
                    Some(entry.key_span.clone()),
                )
                .with_path(path.to_string()),
            );
        }
    }
}

fn parse_dependency_section(
    section_entry: &Entry,
    section: PubspecDependencySection,
    syntax: &PubspecSyntaxCheck,
    path: &str,
    analysis: &mut PubspecAnalysis,
) {
    let Some(entries) = mapping(&section_entry.value) else {
        return;
    };
    for entry in entries {
        let Some(version_or_source) = dependency_source(entry, syntax, path, analysis) else {
            continue;
        };
        let source = version_or_source
            .as_deref()
            .map(parse_normalized_dependency_source);
        analysis.dependencies.push(PubspecDependency::new(
            entry.key.clone(),
            section,
            version_or_source,
            source,
            entry.key_span.clone(),
        ));
    }
}

fn dependency_source(
    entry: &Entry,
    syntax: &PubspecSyntaxCheck,
    path: &str,
    analysis: &mut PubspecAnalysis,
) -> Option<Option<String>> {
    match &entry.value.kind {
        NodeKind::Scalar(value) => {
            if syntax.is_bare_wildcard_line(entry.key_span.start_line) {
                Some(Some("*".to_string()))
            } else {
                Some(Some(value.clone()))
            }
        }
        NodeKind::Mapping(entries) => {
            let mut fields = BTreeMap::new();
            if flatten_fields(entries, "", &mut fields, path, analysis) {
                Some(normalize_dependency_source(&fields))
            } else {
                None
            }
        }
        NodeKind::Unsupported => None,
        NodeKind::Sequence(_) => {
            analysis.diagnostics.push(
                DartDiagnostic::error(
                    "pubspec_invalid_yaml",
                    "dependency sources must be scalar values or mappings",
                    Some(entry.key_span.clone()),
                )
                .with_path(path.to_string()),
            );
            None
        }
    }
}

fn flatten_fields(
    entries: &[Entry],
    prefix: &str,
    fields: &mut BTreeMap<String, String>,
    path: &str,
    analysis: &mut PubspecAnalysis,
) -> bool {
    let mut valid = true;
    for entry in entries {
        let field_path = if prefix.is_empty() {
            entry.key.clone()
        } else {
            format!("{prefix}.{}", entry.key)
        };
        match &entry.value.kind {
            NodeKind::Scalar(value) => {
                fields.insert(field_path, value.clone());
            }
            NodeKind::Mapping(children) => {
                valid &= flatten_fields(children, &field_path, fields, path, analysis);
            }
            NodeKind::Unsupported => valid = false,
            NodeKind::Sequence(_) => {
                analysis.diagnostics.push(
                    DartDiagnostic::error(
                        "pubspec_invalid_yaml",
                        "dependency source fields must be scalar values or mappings",
                        Some(entry.key_span.clone()),
                    )
                    .with_path(path.to_string()),
                );
                valid = false;
            }
        }
    }
    valid
}

fn normalize_dependency_source(fields: &BTreeMap<String, String>) -> Option<String> {
    if fields.is_empty() {
        return None;
    }
    if fields
        .get("workspace")
        .is_some_and(|value| matches!(value.as_str(), "true" | "yes" | "on"))
    {
        return Some("workspace".to_string());
    }
    if let Some(value) = fields.get("sdk") {
        return Some(format!("sdk:{value}"));
    }
    if let Some(value) = fields.get("path") {
        return Some(format!("path:{value}"));
    }
    if fields.contains_key("git") || fields.keys().any(|key| key.starts_with("git.")) {
        return Some(format_source_fields("git", fields));
    }
    if fields.contains_key("hosted") || fields.keys().any(|key| key.starts_with("hosted.")) {
        return Some(format_source_fields("hosted", fields));
    }
    if fields.len() == 1 {
        if let Some(version) = fields.get("version") {
            return Some(version.clone());
        }
    }
    Some(
        fields
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

fn format_source_fields(kind: &str, fields: &BTreeMap<String, String>) -> String {
    let mut parts = Vec::new();
    for (key, value) in fields {
        if key == kind {
            parts.push(value.clone());
        } else if let Some(suffix) = key.strip_prefix(&format!("{kind}.")) {
            parts.push(format!("{suffix}={value}"));
        } else if key == "version" {
            parts.push(format!("version={value}"));
        }
    }
    if parts.is_empty() {
        kind.to_string()
    } else {
        format!("{kind}:{}", parts.join(";"))
    }
}

fn scalar_value(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Scalar(value) => Some(value),
        _ => None,
    }
}

fn mapping(node: &Node) -> Option<&[Entry]> {
    match &node.kind {
        NodeKind::Mapping(entries) => Some(entries),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubspec_syntax::prepare_pubspec_source;

    fn parse(source: &str) -> PubspecAnalysis {
        let prepared = prepare_pubspec_source(source);
        parse_pubspec(
            PubspecInput::new("config\\pubspec.yaml", prepared.source),
            &prepared.syntax,
        )
    }

    #[test]
    fn parses_package_name_and_all_dependency_source_shapes() {
        let analysis = parse(concat!(
            "name: demo\n",
            "dependencies:\n",
            "  flutter:\n",
            "    sdk: flutter\n",
            "  local_package:\n",
            "    path: ../local_package\n",
            "  remote_package:\n",
            "    git:\n",
            "      url: https://example.com/repo.git\n",
            "      ref: stable\n",
            "    version: ^1.0.0\n",
            "  hosted_package:\n",
            "    hosted:\n",
            "      name: hosted_package\n",
            "      url: https://pub.example.com\n",
            "    version: ^2.0.0\n",
            "  workspace_package:\n",
            "    workspace: true\n",
            "dev_dependencies:\n",
            "  test: ^1.25.0\n",
            "dependency_overrides:\n",
            "  local_package: { path: ../override }\n",
        ));

        assert_eq!(analysis.path, "config/pubspec.yaml");
        assert_eq!(analysis.package_name.as_deref(), Some("demo"));
        assert_eq!(analysis.dependencies.len(), 7);
        assert_eq!(source_for(&analysis, "flutter"), Some("sdk:flutter"));
        assert_eq!(
            source_for(&analysis, "remote_package"),
            Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0")
        );
        assert_eq!(
            source_for(&analysis, "hosted_package"),
            Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0")
        );
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn preserves_duplicate_entries_and_unicode_key_bytes() {
        let source = concat!(
            "name: демо\r\n",
            "description: Привет\r\n",
            "dependencies:\r\n",
            "  пакет: ^1.0.0\r\n",
            "  пакет: ^2.0.0\r\n",
        );
        let analysis = parse(source);
        let expected = source.find("  пакет: ^1.0.0").expect("dependency") + 2;

        assert_eq!(analysis.dependencies.len(), 2);
        assert_eq!(analysis.dependencies[0].span.byte_start, expected);
        assert_eq!(analysis.dependencies[0].span.start_column, 3);
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "pubspec_duplicate_key"));
    }

    fn source_for<'a>(analysis: &'a PubspecAnalysis, name: &str) -> Option<&'a str> {
        analysis
            .dependencies
            .iter()
            .find(|dependency| dependency.name == name)
            .and_then(|dependency| dependency.version_or_source.as_deref())
    }
}
