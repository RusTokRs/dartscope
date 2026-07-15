use std::collections::BTreeMap;

use dartscope_core::{
    normalize_path, DartDiagnostic, PubspecAnalysis, PubspecDependency, PubspecDependencySection,
    PubspecInput, SourceSpan,
};

use crate::source_lines::{attach_diagnostic_paths, source_lines};

pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let path = normalize_path(input.path);
    let mut analysis = PubspecAnalysis {
        path: path.clone(),
        package_name: None,
        dependencies: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut state = PubspecParseState::default();

    for source_line in source_lines(&input.source) {
        let line = source_line.text;
        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);

        if leading_indentation_contains_tab(line) {
            analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_indentation",
                "pubspec.yaml indentation must use spaces, not tabs",
                Some(span),
            ));
            continue;
        }

        let yaml = strip_yaml_comment(line);
        let trimmed = yaml.trim();
        if trimmed.is_empty() {
            continue;
        }

        if contains_unsupported_alias_syntax(trimmed) {
            analysis.diagnostics.push(DartDiagnostic::warning(
                "pubspec_unsupported_yaml_alias",
                "YAML anchors, aliases, and merge keys are not supported by the pubspec parser",
                Some(span.clone()),
            ));
        }

        let indent = leading_space_count(line);
        if indent == 0 {
            state.finish_dependency(&mut analysis.dependencies);
            parse_top_level_line(trimmed, span, &mut analysis, &mut state);
        } else if state.section.is_some() {
            parse_dependency_line(trimmed, indent, span, &mut analysis, &mut state);
        }
    }

    state.finish_dependency(&mut analysis.dependencies);

    if analysis.package_name.is_none() {
        analysis.diagnostics.push(DartDiagnostic::warning(
            "pubspec_missing_name",
            "pubspec.yaml does not declare a package name",
            None,
        ));
    }

    attach_diagnostic_paths(&mut analysis.diagnostics, &analysis.path);
    analysis
}

#[derive(Default)]
struct PubspecParseState {
    section: Option<PubspecDependencySection>,
    dependency_indent: Option<usize>,
    dependency: Option<DependencyBuilder>,
    nested_keys: Vec<NestedKey>,
}

impl PubspecParseState {
    fn finish_dependency(&mut self, dependencies: &mut Vec<PubspecDependency>) {
        if let Some(dependency) = self.dependency.take() {
            dependencies.push(dependency.finish());
        }
        self.nested_keys.clear();
    }
}

struct DependencyBuilder {
    name: String,
    section: PubspecDependencySection,
    scalar: Option<String>,
    fields: BTreeMap<String, String>,
    span: SourceSpan,
}

impl DependencyBuilder {
    fn new(
        name: &str,
        section: PubspecDependencySection,
        scalar: Option<&str>,
        span: SourceSpan,
    ) -> Result<Self, ()> {
        let mut builder = Self {
            name: yaml_scalar(name).to_string(),
            section,
            scalar: None,
            fields: BTreeMap::new(),
            span,
        };

        match scalar {
            Some(value) if value.trim_start().starts_with('{') => {
                if !parse_flow_mapping_fields(value, "", &mut builder.fields) {
                    return Err(());
                }
            }
            Some(value) => builder.scalar = Some(yaml_scalar(value).to_string()),
            None => {}
        }

        Ok(builder)
    }

    fn insert_field(&mut self, path: String, value: &str) {
        self.fields.insert(path, yaml_scalar(value).to_string());
    }

    fn finish(self) -> PubspecDependency {
        PubspecDependency {
            name: self.name,
            section: self.section,
            version_or_source: normalize_dependency_source(self.scalar, &self.fields),
            span: self.span,
        }
    }
}

struct NestedKey {
    indent: usize,
    key: String,
}

fn parse_top_level_line(
    trimmed: &str,
    span: SourceSpan,
    analysis: &mut PubspecAnalysis,
    state: &mut PubspecParseState,
) {
    if matches!(trimmed, "---" | "...") {
        analysis.diagnostics.push(DartDiagnostic::warning(
            "pubspec_multiple_documents_unsupported",
            "pubspec.yaml must contain a single YAML document",
            Some(span),
        ));
        state.section = None;
        state.dependency_indent = None;
        return;
    }

    let Some((key, value)) = yaml_key_value(trimmed) else {
        analysis.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_yaml",
            "expected a top-level YAML mapping entry",
            Some(span),
        ));
        state.section = None;
        state.dependency_indent = None;
        return;
    };

    state.section = match (key, value) {
        ("dependencies", None) => Some(PubspecDependencySection::Dependencies),
        ("dev_dependencies", None) => Some(PubspecDependencySection::DevDependencies),
        ("dependency_overrides", None) => Some(PubspecDependencySection::DependencyOverrides),
        _ => None,
    };
    state.dependency_indent = None;

    if key == "name" {
        match value {
            Some(value) => analysis.package_name = Some(yaml_scalar(value).to_string()),
            None => analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_name",
                "the pubspec package name must be a scalar value",
                Some(span),
            )),
        }
    }
}

fn parse_dependency_line(
    trimmed: &str,
    indent: usize,
    span: SourceSpan,
    analysis: &mut PubspecAnalysis,
    state: &mut PubspecParseState,
) {
    let direct_indent = *state.dependency_indent.get_or_insert(indent);
    if indent < direct_indent {
        state.finish_dependency(&mut analysis.dependencies);
        state.section = None;
        state.dependency_indent = None;
        analysis.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_indentation",
            "dependency entry is indented less than the dependency section",
            Some(span),
        ));
        return;
    }

    let Some((key, value)) = yaml_key_value(trimmed) else {
        analysis.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_yaml",
            "expected a dependency mapping entry",
            Some(span),
        ));
        return;
    };

    if indent == direct_indent {
        state.finish_dependency(&mut analysis.dependencies);
        let key_span = mapping_key_span(&span, indent, trimmed);
        state.dependency = match state.section {
            Some(section) => match DependencyBuilder::new(key, section, value, key_span) {
                Ok(dependency) => Some(dependency),
                Err(()) => {
                    analysis.diagnostics.push(DartDiagnostic::error(
                        "pubspec_invalid_yaml",
                        "invalid inline dependency source mapping",
                        Some(span),
                    ));
                    None
                }
            },
            None => None,
        };
        return;
    }

    let Some(dependency) = state.dependency.as_mut() else {
        analysis.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_indentation",
            "nested dependency source appears before a dependency key",
            Some(span),
        ));
        return;
    };

    while state
        .nested_keys
        .last()
        .is_some_and(|nested| nested.indent >= indent)
    {
        state.nested_keys.pop();
    }

    if let Some(value) = value {
        let mut path = state
            .nested_keys
            .iter()
            .map(|nested| nested.key.as_str())
            .collect::<Vec<_>>();
        path.push(key);
        dependency.insert_field(path.join("."), value);
    } else {
        state.nested_keys.push(NestedKey {
            indent,
            key: key.to_string(),
        });
    }
}

fn mapping_key_span(line_span: &SourceSpan, indent: usize, trimmed: &str) -> SourceSpan {
    let key_end = find_unquoted_colon(trimmed).unwrap_or(trimmed.len());
    let raw_key = trimmed[..key_end].trim_end();
    SourceSpan {
        byte_start: line_span.byte_start + indent,
        byte_end: line_span.byte_start + indent + raw_key.len(),
        start_line: line_span.start_line,
        start_column: indent + 1,
        end_line: line_span.start_line,
        end_column: indent + raw_key.chars().count() + 1,
    }
}

fn parse_flow_mapping_fields(
    value: &str,
    prefix: &str,
    fields: &mut BTreeMap<String, String>,
) -> bool {
    let value = value.trim();
    let Some(inner) = value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return false;
    };

    for entry in split_flow_entries(inner) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = yaml_key_value(entry) else {
            return false;
        };
        let Some(value) = value else {
            return false;
        };
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        if value.trim_start().starts_with('{') {
            if !parse_flow_mapping_fields(value, &path, fields) {
                return false;
            }
        } else {
            fields.insert(path, yaml_scalar(value).to_string());
        }
    }

    true
}

fn split_flow_entries(value: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;

    for (index, ch) in value.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(&value[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    entries.push(&value[start..]);
    entries
}

fn normalize_dependency_source(
    scalar: Option<String>,
    fields: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(scalar) = scalar {
        return Some(scalar);
    }
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

fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let colon = find_unquoted_colon(trimmed)?;
    let key = trimmed[..colon].trim();
    if key.is_empty() || key.starts_with('-') {
        return None;
    }
    let value = trimmed[colon + 1..].trim();
    Some((yaml_scalar(key), (!value.is_empty()).then_some(value)))
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

fn strip_yaml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;

    for (index, ch) in line.char_indices() {
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
                '#' if previous.is_none_or(char::is_whitespace) => return &line[..index],
                _ => {}
            }
        }
        previous = Some(ch);
    }

    line
}

fn contains_unsupported_alias_syntax(value: &str) -> bool {
    if value.starts_with("<<:") {
        return true;
    }

    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;
    for ch in value.chars() {
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
                '&' | '*'
                    if previous.is_none_or(|previous: char| {
                        previous.is_whitespace() || previous == ':'
                    }) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        previous = Some(ch);
    }
    false
}

fn leading_indentation_contains_tab(line: &str) -> bool {
    line.chars()
        .take_while(|ch| ch.is_whitespace())
        .any(|ch| ch == '\t')
}

fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

fn yaml_scalar(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if matches!((first, last), (b'\'', b'\'') | (b'"', b'"')) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_nested_dependency_sources_without_inventing_packages() {
        let source = r#"name: demo
dependencies:
  flutter:
    sdk: flutter
  local_package:
    path: ../local_package
  remote_package:
    git:
      url: https://example.com/repo.git
      ref: stable
    version: ^1.0.0
  hosted_package:
    hosted:
      name: hosted_package
      url: https://pub.example.com
    version: ^2.0.0
  workspace_package:
    workspace: true
"#;

        let analysis = parse_pubspec(PubspecInput::new("packages\\demo\\pubspec.yaml", source));

        assert_eq!(analysis.path, "packages/demo/pubspec.yaml");
        assert_eq!(analysis.dependencies.len(), 5);
        assert_eq!(source_for(&analysis, "flutter"), Some("sdk:flutter"));
        assert_eq!(
            source_for(&analysis, "local_package"),
            Some("path:../local_package")
        );
        assert_eq!(
            source_for(&analysis, "remote_package"),
            Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0")
        );
        assert_eq!(
            source_for(&analysis, "hosted_package"),
            Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0")
        );
        assert_eq!(source_for(&analysis, "workspace_package"), Some("workspace"));
        assert!(!analysis.dependencies.iter().any(|dependency| matches!(
            dependency.name.as_str(),
            "sdk" | "path" | "git" | "url" | "ref"
        )));
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn parses_inline_dependency_source_mappings() {
        let source = r#"name: demo
dependencies:
  local_package: { path: ../local_package }
  remote_package: { git: { url: https://example.com/repo.git, ref: stable }, version: ^1.0.0 }
  hosted_package: { hosted: { name: hosted_package, url: https://pub.example.com }, version: ^2.0.0 }
  workspace_package: { workspace: true }
"#;

        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.dependencies.len(), 4);
        assert_eq!(
            source_for(&analysis, "local_package"),
            Some("path:../local_package")
        );
        assert_eq!(
            source_for(&analysis, "remote_package"),
            Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0")
        );
        assert_eq!(
            source_for(&analysis, "hosted_package"),
            Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0")
        );
        assert_eq!(source_for(&analysis, "workspace_package"), Some("workspace"));
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn preserves_dependency_key_spans_with_comments_and_crlf() {
        let source = "name: demo\r\ndependencies: # packages\r\n    http: ^1.2.0 # client\r\n";
        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));
        let dependency = analysis
            .dependencies
            .iter()
            .find(|dependency| dependency.name == "http")
            .expect("http dependency");

        let expected_start = "name: demo\r\ndependencies: # packages\r\n".len() + 4;
        assert_eq!(dependency.span.start_line, 3);
        assert_eq!(dependency.span.start_column, 5);
        assert_eq!(dependency.span.end_column, 9);
        assert_eq!(dependency.span.byte_start, expected_start);
        assert_eq!(dependency.span.byte_end, expected_start + "http".len());
        assert_eq!(dependency.version_or_source.as_deref(), Some("^1.2.0"));
    }

    #[test]
    fn diagnoses_aliases_tabs_and_malformed_entries_with_paths() {
        let source = concat!(
            "name: demo\n",
            "dependencies:\n",
            "\tbad: any\n",
            "  defaults: &defaults\n",
            "    path: ../defaults\n",
            "  merged:\n",
            "    <<: *defaults\n",
            "  broken entry\n",
        );
        let analysis = parse_pubspec(PubspecInput::new("config\\pubspec.yaml", source));

        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_indentation"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_unsupported_yaml_alias"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "pubspec_invalid_yaml"));
    }

    #[test]
    fn diagnoses_malformed_inline_dependency_source() {
        let source = concat!(
            "name: demo\n",
            "dependencies:\n",
            "  broken: { git: { url: https://example.com/repo.git }\n",
        );
        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert!(analysis.dependencies.is_empty());
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_yaml"
                && diagnostic.path.as_deref() == Some("pubspec.yaml")
        }));
    }

    fn source_for<'a>(analysis: &'a PubspecAnalysis, name: &str) -> Option<&'a str> {
        analysis
            .dependencies
            .iter()
            .find(|dependency| dependency.name == name)
            .and_then(|dependency| dependency.version_or_source.as_deref())
    }
}
