use dartscope_core::{
    normalize_path, Confidence, DartDeclaration, DartDeclarationKind, DartDiagnostic, DartExport,
    DartFileAnalysis, DartFileInput, DartImport, DartPart, DartPartOf, FlutterWidgetHint,
    PubspecAnalysis, PubspecDependency, PubspecDependencySection, PubspecInput, SourceSpan,
};

pub fn analyze_file(input: DartFileInput) -> DartFileAnalysis {
    let mut analysis = DartFileAnalysis::empty(input.path);
    let mut byte_offset = 0usize;

    for (index, line) in input.source.lines().enumerate() {
        let line_number = index + 1;
        let span = SourceSpan::line(line_number, byte_offset, line);
        let trimmed = line.trim();

        if trimmed.contains("<<<<<<<") || trimmed.contains(">>>>>>>") {
            analysis.diagnostics.push(DartDiagnostic::warning(
                "merge_conflict_marker",
                "source contains a merge conflict marker",
                Some(span.clone()),
            ));
        }

        if let Some(uri) = directive_uri(trimmed, "import") {
            if uri == "package:flutter/material.dart" || uri == "package:flutter/widgets.dart" {
                analysis.flutter.imports_flutter = true;
            }
            analysis.imports.push(DartImport {
                uri,
                span: span.clone(),
            });
        } else if let Some(uri) = directive_uri(trimmed, "export") {
            analysis.exports.push(DartExport {
                uri,
                span: span.clone(),
            });
        } else if let Some(uri) = directive_uri(trimmed, "part") {
            analysis.parts.push(DartPart {
                uri,
                span: span.clone(),
            });
        } else if let Some(library) = part_of_value(trimmed) {
            analysis.part_of = Some(DartPartOf {
                library,
                span: span.clone(),
            });
        } else if let Some(declaration) = declaration_from_line(trimmed, span.clone()) {
            if let Some(base_class) = declaration
                .extends
                .clone()
                .filter(|base| is_flutter_base(base))
            {
                analysis.flutter.widgets.push(FlutterWidgetHint {
                    class_name: declaration.name.clone(),
                    base_class,
                    confidence: Confidence::High,
                    span: span.clone(),
                });
            }
            analysis.declarations.push(declaration);
        }

        if directive_like_without_semicolon(trimmed) {
            analysis.diagnostics.push(DartDiagnostic::warning(
                "directive_missing_semicolon",
                "Dart import/export/part directive appears to be missing a semicolon",
                Some(span),
            ));
        }

        byte_offset += line.len() + 1;
    }

    analysis
}

pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let mut analysis = PubspecAnalysis {
        path: normalize_path(input.path),
        package_name: None,
        dependencies: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut section: Option<PubspecDependencySection> = None;
    let mut byte_offset = 0usize;

    for (index, line) in input.source.lines().enumerate() {
        let line_number = index + 1;
        let span = SourceSpan::line(line_number, byte_offset, line);
        let trimmed = line.trim();
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            byte_offset += line.len() + 1;
            continue;
        }

        if indent == 0 {
            section = match trimmed.trim_end_matches(':') {
                "dependencies" => Some(PubspecDependencySection::Dependencies),
                "dev_dependencies" => Some(PubspecDependencySection::DevDependencies),
                "dependency_overrides" => Some(PubspecDependencySection::DependencyOverrides),
                _ => None,
            };
            if let Some(value) = key_value(trimmed, "name") {
                analysis.package_name = Some(value.to_string());
            }
        } else if let Some(section) = section.filter(|_| indent <= 2) {
            if let Some((name, value)) = yaml_key_value(trimmed) {
                analysis.dependencies.push(PubspecDependency {
                    name: name.to_string(),
                    section,
                    version_or_source: value.map(str::to_string),
                    span,
                });
            }
        }

        byte_offset += line.len() + 1;
    }

    if analysis.package_name.is_none() {
        analysis.diagnostics.push(DartDiagnostic::warning(
            "pubspec_missing_name",
            "pubspec.yaml does not declare a package name",
            None,
        ));
    }

    analysis
}

fn directive_uri(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    quoted_value(rest)
}

fn part_of_value(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("part of")?.trim();
    quoted_value(rest).or_else(|| {
        rest.trim_end_matches(';')
            .split_whitespace()
            .next()
            .map(str::to_string)
    })
}

fn quoted_value(input: &str) -> Option<String> {
    let quote = input.find(['\'', '"'])?;
    let quote_char = input.as_bytes()[quote] as char;
    let rest = &input[quote + 1..];
    let end = rest.find(quote_char)?;
    Some(rest[..end].to_string())
}

fn declaration_from_line(trimmed: &str, span: SourceSpan) -> Option<DartDeclaration> {
    if let Some(name) = name_after_keyword(trimmed, "class") {
        return Some(DartDeclaration {
            name,
            kind: DartDeclarationKind::Class,
            span,
            extends: value_after_keyword(trimmed, "extends"),
            mixes_in: values_after_keyword(trimmed, "with"),
        });
    }
    if let Some(name) = name_after_keyword(trimmed, "mixin") {
        return Some(simple_declaration(name, DartDeclarationKind::Mixin, span));
    }
    if let Some(name) = name_after_keyword(trimmed, "enum") {
        return Some(simple_declaration(name, DartDeclarationKind::Enum, span));
    }
    if let Some(name) = name_after_keyword(trimmed, "extension") {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::Extension,
            span,
        ));
    }
    if let Some(name) = name_after_keyword(trimmed, "typedef") {
        return Some(simple_declaration(name, DartDeclarationKind::Typedef, span));
    }
    top_level_function(trimmed)
        .map(|name| simple_declaration(name, DartDeclarationKind::Function, span))
}

fn simple_declaration(
    name: String,
    kind: DartDeclarationKind,
    span: SourceSpan,
) -> DartDeclaration {
    DartDeclaration {
        name,
        kind,
        span,
        extends: None,
        mixes_in: Vec::new(),
    }
}

fn name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    next_identifier(rest)
}

fn value_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let marker = format!(" {keyword} ");
    let index = trimmed.find(&marker)?;
    next_identifier(&trimmed[index + marker.len()..])
}

fn values_after_keyword(trimmed: &str, keyword: &str) -> Vec<String> {
    let marker = format!(" {keyword} ");
    let Some(index) = trimmed.find(&marker) else {
        return Vec::new();
    };
    trimmed[index + marker.len()..]
        .split(['{', '('])
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| next_identifier(part.trim()))
        .collect()
}

fn next_identifier(input: &str) -> Option<String> {
    let ident: String = input
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    (!ident.is_empty()).then_some(ident)
}

fn top_level_function(trimmed: &str) -> Option<String> {
    if !trimmed.ends_with('{') && !trimmed.ends_with("=>") && !trimmed.contains('(') {
        return None;
    }
    if trimmed.starts_with("if ") || trimmed.starts_with("for ") || trimmed.starts_with("while ") {
        return None;
    }
    let before_paren = trimmed.split_once('(')?.0.trim();
    let name = before_paren.split_whitespace().last()?;
    is_identifier(name).then_some(name.to_string())
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_flutter_base(base: &str) -> bool {
    matches!(base, "Widget" | "StatelessWidget" | "StatefulWidget")
}

fn directive_like_without_semicolon(trimmed: &str) -> bool {
    (trimmed.starts_with("import ")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("part ")
        || trimmed.starts_with("part of "))
        && !trimmed.ends_with(';')
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

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::{DartDeclarationKind, PubspecDependencySection};

    #[test]
    fn analyzes_dart_imports_parts_declarations_and_flutter_widgets() {
        let source = r#"
import 'package:flutter/material.dart';
import 'src/model.dart';
export "src/api.dart";
part 'home.g.dart';

class HomeScreen extends StatelessWidget {
}

typedef Mapper = String Function(int value);
"#;

        let analysis = analyze_file(DartFileInput::new("lib\\home.dart", source));

        assert_eq!(analysis.path, "lib/home.dart");
        assert_eq!(analysis.imports.len(), 2);
        assert_eq!(analysis.exports[0].uri, "src/api.dart");
        assert_eq!(analysis.parts[0].uri, "home.g.dart");
        assert!(analysis.flutter.imports_flutter);
        assert_eq!(analysis.flutter.widgets[0].class_name, "HomeScreen");
        assert!(analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "Mapper"
                && declaration.kind == DartDeclarationKind::Typedef));
    }

    #[test]
    fn parses_pubspec_dependencies() {
        let source = r#"
name: demo_app
dependencies:
  flutter:
    sdk: flutter
  http: ^1.2.0
dev_dependencies:
  test: ^1.25.0
"#;

        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.package_name.as_deref(), Some("demo_app"));
        assert!(analysis.dependencies.iter().any(|dependency| {
            dependency.name == "http"
                && dependency.section == PubspecDependencySection::Dependencies
                && dependency.version_or_source.as_deref() == Some("^1.2.0")
        }));
        assert!(analysis.dependencies.iter().any(|dependency| {
            dependency.name == "test"
                && dependency.section == PubspecDependencySection::DevDependencies
        }));
    }
}
