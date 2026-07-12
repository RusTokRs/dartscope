use dartscope_core::{
    normalize_path, DartDiagnostic, PubspecAnalysis, PubspecDependency, PubspecDependencySection,
    PubspecInput, SourceSpan,
};

use crate::source_lines::{attach_diagnostic_paths, source_lines};

pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let mut analysis = PubspecAnalysis {
        path: normalize_path(input.path),
        package_name: None,
        dependencies: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut section: Option<PubspecDependencySection> = None;
    let mut dependency_indent: Option<usize> = None;

    for source_line in source_lines(&input.source) {
        let line = source_line.text;
        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);
        let yaml = strip_yaml_comment(line);
        let trimmed = yaml.trim();
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();

        if trimmed.is_empty() {
            continue;
        }

        if indent == 0 {
            let (key, value) = yaml_key_value(trimmed).unwrap_or((trimmed, None));
            section = match (key, value) {
                ("dependencies", None) => Some(PubspecDependencySection::Dependencies),
                ("dev_dependencies", None) => Some(PubspecDependencySection::DevDependencies),
                ("dependency_overrides", None) => {
                    Some(PubspecDependencySection::DependencyOverrides)
                }
                _ => None,
            };
            dependency_indent = None;
            if let Some(value) = key_value(trimmed, "name") {
                analysis.package_name = Some(yaml_scalar(value).to_string());
            }
        } else if let Some(section) = section {
            if let Some((name, value)) = yaml_key_value(trimmed) {
                let direct_indent = *dependency_indent.get_or_insert(indent);
                if indent == direct_indent {
                    analysis.dependencies.push(PubspecDependency {
                        name: name.to_string(),
                        section,
                        version_or_source: value.map(yaml_scalar).map(str::to_string),
                        span,
                    });
                }
            }
        }
    }

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

fn key_value<'a>(trimmed: &'a str, key: &str) -> Option<&'a str> {
    trimmed
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let (key, value) = trimmed.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.starts_with('-') {
        return None;
    }
    let value = value.trim();
    Some((key, (!value.is_empty()).then_some(value)))
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
