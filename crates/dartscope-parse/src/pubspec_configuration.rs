use dartscope_core::{normalize_path, DartDiagnostic, PubspecInput, SourceSpan};
use serde::{Deserialize, Serialize};

use crate::source_lines::{attach_diagnostic_paths, source_lines, SourceLine};

/// Typed pubspec configuration that is not part of dependency discovery.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecConfigurationAnalysis {
    pub path: String,
    pub environment: Vec<PubspecEnvironmentConstraint>,
    pub flutter: PubspecFlutterConfiguration,
    pub diagnostics: Vec<DartDiagnostic>,
}

/// One entry from the top-level `environment` mapping.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecEnvironmentConstraint {
    pub name: String,
    pub constraint: String,
    pub span: SourceSpan,
}

/// Normalized configuration owned by the top-level `flutter` mapping.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecFlutterConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_material_design: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_localizations: Option<bool>,
    pub assets: Vec<PubspecFlutterAsset>,
    pub fonts: Vec<PubspecFlutterFontFamily>,
}

/// A scalar or `path` asset entry from `flutter.assets`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterAsset {
    pub path: String,
    pub span: SourceSpan,
}

/// A family from `flutter.fonts`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterFontFamily {
    pub family: String,
    pub fonts: Vec<PubspecFlutterFont>,
    pub span: SourceSpan,
}

/// A concrete font asset within a Flutter font family.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterFont {
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    pub span: SourceSpan,
}

/// Parses environment constraints and common Flutter pubspec configuration.
pub fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    let path = normalize_path(input.path);
    let mut parser = ConfigurationParser {
        analysis: PubspecConfigurationAnalysis {
            path: path.clone(),
            environment: Vec::new(),
            flutter: PubspecFlutterConfiguration::default(),
            diagnostics: Vec::new(),
        },
        section: Section::None,
        direct_indent: None,
        flutter_subsection: FlutterSubsection::None,
        current_family: None,
        current_font: None,
    };

    for line in source_lines(&input.source) {
        parser.observe(line);
    }
    parser.finish_fonts();
    attach_diagnostic_paths(&mut parser.analysis.diagnostics, &path);
    parser.analysis
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Section {
    None,
    Environment,
    Flutter,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum FlutterSubsection {
    None,
    Assets,
    Fonts,
}

struct ConfigurationParser {
    analysis: PubspecConfigurationAnalysis,
    section: Section,
    direct_indent: Option<usize>,
    flutter_subsection: FlutterSubsection,
    current_family: Option<PubspecFlutterFontFamily>,
    current_font: Option<PubspecFlutterFont>,
}

impl ConfigurationParser {
    fn observe(&mut self, source_line: SourceLine<'_>) {
        let line = source_line.text;
        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);
        if leading_indentation_contains_tab(line) {
            self.analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_indentation",
                "pubspec.yaml indentation must use spaces, not tabs",
                Some(span),
            ));
            return;
        }

        let yaml = strip_yaml_comment(line);
        let trimmed = yaml.trim();
        if trimmed.is_empty() {
            return;
        }
        let indent = leading_space_count(line);
        if indent == 0 {
            self.finish_fonts();
            self.observe_top_level(trimmed);
            return;
        }

        match self.section {
            Section::Environment => self.observe_environment(trimmed, indent, span),
            Section::Flutter => self.observe_flutter(trimmed, indent, span),
            Section::None => {}
        }
    }

    fn observe_top_level(&mut self, trimmed: &str) {
        self.direct_indent = None;
        self.flutter_subsection = FlutterSubsection::None;
        self.section = match yaml_key_value(trimmed) {
            Some(("environment", None)) => Section::Environment,
            Some(("flutter", None)) => Section::Flutter,
            _ => Section::None,
        };
    }

    fn observe_environment(&mut self, trimmed: &str, indent: usize, span: SourceSpan) {
        let direct_indent = *self.direct_indent.get_or_insert(indent);
        if indent != direct_indent {
            self.analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_environment",
                "environment constraints must be scalar mapping entries",
                Some(span),
            ));
            return;
        }

        let Some((name, Some(constraint))) = yaml_key_value(trimmed) else {
            self.analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_environment",
                "environment constraints must have scalar values",
                Some(span),
            ));
            return;
        };
        self.analysis.environment.push(PubspecEnvironmentConstraint {
            name: yaml_scalar(name).to_string(),
            constraint: yaml_scalar(constraint).to_string(),
            span: mapping_key_span(&span, indent, trimmed),
        });
    }

    fn observe_flutter(&mut self, trimmed: &str, indent: usize, span: SourceSpan) {
        let direct_indent = *self.direct_indent.get_or_insert(indent);
        if indent == direct_indent {
            self.finish_fonts();
            let Some((key, value)) = yaml_key_value(trimmed) else {
                self.analysis.diagnostics.push(DartDiagnostic::error(
                    "pubspec_invalid_flutter_configuration",
                    "expected a Flutter configuration mapping entry",
                    Some(span),
                ));
                return;
            };
            match (key, value) {
                ("assets", None) => self.flutter_subsection = FlutterSubsection::Assets,
                ("fonts", None) => self.flutter_subsection = FlutterSubsection::Fonts,
                ("uses-material-design", Some(value)) => {
                    self.flutter_subsection = FlutterSubsection::None;
                    let parsed = self.parse_flutter_bool(key, value, span);
                    self.analysis.flutter.uses_material_design = parsed;
                }
                ("generate", Some(value)) => {
                    self.flutter_subsection = FlutterSubsection::None;
                    let parsed = self.parse_flutter_bool(key, value, span);
                    self.analysis.flutter.generate_localizations = parsed;
                }
                _ => self.flutter_subsection = FlutterSubsection::None,
            }
            return;
        }

        if indent < direct_indent {
            self.finish_fonts();
            self.section = Section::None;
            self.direct_indent = None;
            self.flutter_subsection = FlutterSubsection::None;
            return;
        }

        match self.flutter_subsection {
            FlutterSubsection::Assets => self.observe_flutter_asset(trimmed, span),
            FlutterSubsection::Fonts => self.observe_flutter_font(trimmed, span),
            FlutterSubsection::None => {}
        }
    }

    fn parse_flutter_bool(
        &mut self,
        key: &str,
        value: &str,
        span: SourceSpan,
    ) -> Option<bool> {
        match yaml_scalar(value) {
            "true" => Some(true),
            "false" => Some(false),
            _ => {
                self.analysis.diagnostics.push(DartDiagnostic::error(
                    "pubspec_invalid_flutter_boolean",
                    format!("flutter.{key} must be true or false"),
                    Some(span),
                ));
                None
            }
        }
    }

    fn observe_flutter_asset(&mut self, trimmed: &str, span: SourceSpan) {
        let Some(item) = trimmed.strip_prefix('-').map(str::trim) else {
            self.analysis.diagnostics.push(DartDiagnostic::warning(
                "pubspec_unsupported_flutter_asset",
                "Flutter asset entries must be list items",
                Some(span),
            ));
            return;
        };
        let path = match yaml_key_value(item) {
            Some(("path", Some(path))) => yaml_scalar(path),
            Some(_) => {
                self.analysis.diagnostics.push(DartDiagnostic::warning(
                    "pubspec_unsupported_flutter_asset",
                    "only scalar assets and asset path mappings are currently supported",
                    Some(span),
                ));
                return;
            }
            None => yaml_scalar(item),
        };
        if path.is_empty() {
            self.analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_flutter_asset",
                "Flutter asset path cannot be empty",
                Some(span),
            ));
            return;
        }
        self.analysis.flutter.assets.push(PubspecFlutterAsset {
            path: path.to_string(),
            span,
        });
    }

    fn observe_flutter_font(&mut self, trimmed: &str, span: SourceSpan) {
        if let Some(item) = trimmed.strip_prefix('-').map(str::trim) {
            let Some((key, Some(value))) = yaml_key_value(item) else {
                self.analysis.diagnostics.push(DartDiagnostic::error(
                    "pubspec_invalid_flutter_font",
                    "Flutter font list entries must contain a scalar mapping",
                    Some(span),
                ));
                return;
            };
            match key {
                "family" => {
                    self.finish_fonts();
                    self.current_family = Some(PubspecFlutterFontFamily {
                        family: yaml_scalar(value).to_string(),
                        fonts: Vec::new(),
                        span,
                    });
                }
                "asset" => {
                    self.finish_font();
                    if self.current_family.is_none() {
                        self.analysis.diagnostics.push(DartDiagnostic::error(
                            "pubspec_invalid_flutter_font",
                            "Flutter font asset appears before a font family",
                            Some(span),
                        ));
                        return;
                    }
                    self.current_font = Some(PubspecFlutterFont {
                        asset: yaml_scalar(value).to_string(),
                        style: None,
                        weight: None,
                        span,
                    });
                }
                _ => self.analysis.diagnostics.push(DartDiagnostic::warning(
                    "pubspec_unsupported_flutter_font",
                    "unsupported Flutter font list entry",
                    Some(span),
                )),
            }
            return;
        }

        let Some((key, value)) = yaml_key_value(trimmed) else {
            self.analysis.diagnostics.push(DartDiagnostic::error(
                "pubspec_invalid_flutter_font",
                "expected a Flutter font mapping entry",
                Some(span),
            ));
            return;
        };
        match (key, value) {
            ("fonts", None) => {}
            ("style", Some(value)) => {
                if let Some(font) = self.current_font.as_mut() {
                    font.style = Some(yaml_scalar(value).to_string());
                } else {
                    self.missing_font_property_target(key, span);
                }
            }
            ("weight", Some(value)) => {
                if let Some(font) = self.current_font.as_mut() {
                    match yaml_scalar(value).parse::<u16>() {
                        Ok(weight) if (100..=900).contains(&weight) && weight % 100 == 0 => {
                            font.weight = Some(weight);
                        }
                        _ => self.analysis.diagnostics.push(DartDiagnostic::error(
                            "pubspec_invalid_flutter_font_weight",
                            "Flutter font weight must be one of 100, 200, ..., 900",
                            Some(span),
                        )),
                    }
                } else {
                    self.missing_font_property_target(key, span);
                }
            }
            _ => {}
        }
    }

    fn missing_font_property_target(&mut self, key: &str, span: SourceSpan) {
        self.analysis.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_flutter_font",
            format!("Flutter font {key} appears before a font asset"),
            Some(span),
        ));
    }

    fn finish_font(&mut self) {
        let Some(font) = self.current_font.take() else {
            return;
        };
        if let Some(family) = self.current_family.as_mut() {
            family.fonts.push(font);
        }
    }

    fn finish_fonts(&mut self) {
        self.finish_font();
        if let Some(family) = self.current_family.take() {
            self.analysis.flutter.fonts.push(family);
        }
    }
}

fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let colon = find_unquoted_colon(trimmed)?;
    let key = trimmed[..colon].trim();
    if key.is_empty() {
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

fn leading_indentation_contains_tab(line: &str) -> bool {
    line.chars()
        .take_while(|ch| ch.is_whitespace())
        .any(|ch| ch == '\t')
}

fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_environment_assets_fonts_and_localization_generation() {
        let source = r#"name: demo
environment:
  sdk: '>=3.4.0 <4.0.0'
  flutter: '>=3.22.0'
flutter:
  uses-material-design: true
  generate: true
  assets:
    - assets/images/
    - path: assets/config/app.json
  fonts:
    - family: Inter
      fonts:
        - asset: fonts/Inter-Regular.ttf
        - asset: fonts/Inter-Bold.ttf
          weight: 700
          style: normal
"#;
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "packages\\demo\\pubspec.yaml",
            source,
        ));

        assert_eq!(analysis.path, "packages/demo/pubspec.yaml");
        assert_eq!(analysis.environment.len(), 2);
        assert_eq!(analysis.environment[0].name, "sdk");
        assert_eq!(analysis.environment[0].constraint, ">=3.4.0 <4.0.0");
        assert_eq!(analysis.flutter.uses_material_design, Some(true));
        assert_eq!(analysis.flutter.generate_localizations, Some(true));
        assert_eq!(analysis.flutter.assets.len(), 2);
        assert_eq!(analysis.flutter.assets[1].path, "assets/config/app.json");
        assert_eq!(analysis.flutter.fonts.len(), 1);
        assert_eq!(analysis.flutter.fonts[0].family, "Inter");
        assert_eq!(analysis.flutter.fonts[0].fonts.len(), 2);
        assert_eq!(analysis.flutter.fonts[0].fonts[1].weight, Some(700));
        assert_eq!(analysis.flutter.fonts[0].fonts[1].style.as_deref(), Some("normal"));
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn preserves_environment_key_spans_for_crlf_input() {
        let source = "name: demo\r\nenvironment:\r\n    sdk: ^3.4.0 # comment\r\n";
        let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));
        let constraint = &analysis.environment[0];
        let expected_start = "name: demo\r\nenvironment:\r\n".len() + 4;

        assert_eq!(constraint.span.start_line, 3);
        assert_eq!(constraint.span.start_column, 5);
        assert_eq!(constraint.span.byte_start, expected_start);
        assert_eq!(constraint.span.byte_end, expected_start + "sdk".len());
    }

    #[test]
    fn diagnoses_invalid_flutter_values_with_normalized_paths() {
        let source = concat!(
            "name: demo\n",
            "flutter:\n",
            "  generate: yes\n",
            "  fonts:\n",
            "    - family: Inter\n",
            "      fonts:\n",
            "        - asset: fonts/Inter.ttf\n",
            "          weight: 750\n",
        );
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "config\\pubspec.yaml",
            source,
        ));

        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_boolean"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_font_weight"
                && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
        }));
    }

    #[test]
    fn serializes_typed_configuration_shapes() {
        let analysis = parse_pubspec_configuration(PubspecInput::new(
            "pubspec.yaml",
            "environment:\n  sdk: ^3.4.0\nflutter:\n  assets:\n    - assets/\n",
        ));
        let value = serde_json::to_value(analysis).expect("serialize configuration");

        assert_eq!(value["environment"][0]["name"], "sdk");
        assert_eq!(value["flutter"]["assets"][0]["path"], "assets/");
    }
}
