use std::collections::BTreeMap;

use dartscope::{
    DartLintAnalysis, DartLintConfig, DartLintDiagnostic, DartLintRuleId, DiagnosticSeverity,
    SourceSpan, to_json_pretty,
};
use serde::Serialize;

const SARIF_VERSION: &str = "2.1.0";
const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";

pub(super) fn to_pretty_json(
    analysis: &DartLintAnalysis,
    config: &DartLintConfig,
) -> Result<String, String> {
    to_json_pretty(&SarifLog::from_analysis(analysis, config)).map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
struct SarifLog {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun>,
}

impl SarifLog {
    fn from_analysis(analysis: &DartLintAnalysis, config: &DartLintConfig) -> Self {
        let mut enabled_rules = config.enabled_rules.clone();
        enabled_rules.sort();
        enabled_rules.dedup();
        let rule_indices = enabled_rules
            .iter()
            .enumerate()
            .map(|(index, rule_id)| (*rule_id, index))
            .collect::<BTreeMap<_, _>>();
        let rules = enabled_rules
            .into_iter()
            .map(|rule_id| SarifRule::new(rule_id, configured_severity(config, rule_id)))
            .collect();
        let results = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| SarifResult::new(diagnostic, &rule_indices))
            .collect();

        Self {
            schema: SARIF_SCHEMA,
            version: SARIF_VERSION,
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "DartScope",
                        semantic_version: env!("CARGO_PKG_VERSION"),
                        information_uri: "https://github.com/RusTokRs/dartscope",
                        rules,
                    },
                },
                column_kind: "unicodeCodePoints",
                results,
            }],
        }
    }
}

fn configured_severity(config: &DartLintConfig, rule_id: DartLintRuleId) -> DiagnosticSeverity {
    config
        .severity_overrides
        .iter()
        .rev()
        .find(|override_| override_.rule_id == rule_id)
        .map(|override_| override_.severity)
        .unwrap_or(DiagnosticSeverity::Warning)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRun {
    tool: SarifTool,
    column_kind: &'static str,
    results: Vec<SarifResult>,
}

#[derive(Debug, Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver {
    name: &'static str,
    semantic_version: &'static str,
    information_uri: &'static str,
    rules: Vec<SarifRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule {
    id: &'static str,
    name: &'static str,
    short_description: SarifMessage,
    full_description: SarifMessage,
    default_configuration: SarifConfiguration,
    help_uri: &'static str,
}

impl SarifRule {
    fn new(rule_id: DartLintRuleId, severity: DiagnosticSeverity) -> Self {
        Self {
            id: rule_id.as_str(),
            name: rule_id.short_name(),
            short_description: SarifMessage {
                text: rule_id.title().to_string(),
            },
            full_description: SarifMessage {
                text: rule_id.description().to_string(),
            },
            default_configuration: SarifConfiguration {
                level: sarif_level(severity),
            },
            help_uri: "https://github.com/RusTokRs/dartscope/blob/main/docs/development/lint-rules.md",
        }
    }
}

#[derive(Debug, Serialize)]
struct SarifConfiguration {
    level: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    rule_id: &'static str,
    rule_index: usize,
    level: &'static str,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    related_locations: Vec<SarifRelatedLocation>,
}

impl SarifResult {
    fn new(
        diagnostic: &DartLintDiagnostic,
        rule_indices: &BTreeMap<DartLintRuleId, usize>,
    ) -> Self {
        let rule_index = rule_indices
            .get(&diagnostic.rule_id)
            .copied()
            .unwrap_or_default();
        Self {
            rule_id: diagnostic.rule_id.as_str(),
            rule_index,
            level: sarif_level(diagnostic.severity),
            message: SarifMessage {
                text: diagnostic.message.clone(),
            },
            locations: vec![SarifLocation::new(
                &diagnostic.path,
                diagnostic.span.as_ref(),
            )],
            related_locations: diagnostic
                .related_paths
                .iter()
                .enumerate()
                .map(|(index, path)| SarifRelatedLocation {
                    id: index + 1,
                    message: SarifMessage {
                        text: "Related path".to_string(),
                    },
                    physical_location: SarifPhysicalLocation::new(path, None),
                })
                .collect(),
        }
    }
}

fn sarif_level(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Info => "note",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

#[derive(Debug, Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    physical_location: SarifPhysicalLocation,
}

impl SarifLocation {
    fn new(path: &str, span: Option<&SourceSpan>) -> Self {
        Self {
            physical_location: SarifPhysicalLocation::file_start(path, span),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRelatedLocation {
    id: usize,
    message: SarifMessage,
    physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<SarifRegion>,
}

impl SarifPhysicalLocation {
    /// A location of a finding. Findings about a whole file have no span; they are reported at
    /// the first line, because consumers such as GitHub code scanning need a region to show them.
    fn new(path: &str, span: Option<&SourceSpan>) -> Self {
        Self::with_region(path, span.map(SarifRegion::from))
    }

    fn file_start(path: &str, span: Option<&SourceSpan>) -> Self {
        Self::with_region(
            path,
            Some(span.map_or_else(SarifRegion::first_line, SarifRegion::from)),
        )
    }

    fn with_region(path: &str, region: Option<SarifRegion>) -> Self {
        Self {
            artifact_location: SarifArtifactLocation {
                uri: uri_reference(path),
            },
            region,
        }
    }
}

/// Encodes a project-relative path as an RFC 3986 URI reference.
///
/// Everything outside the unreserved characters and `/` is percent-encoded byte by byte, so a
/// name such as `a#b.dart` or `файл имя.dart` is not read as a fragment, a scheme or a malformed
/// reference by SARIF consumers.
fn uri_reference(path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let path = path.replace('\\', "/");
    let mut uri = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => {
                uri.push('%');
                uri.push(char::from(HEX[usize::from(byte >> 4)]));
                uri.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    uri
}

#[derive(Debug, Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

impl SarifRegion {
    fn first_line() -> Self {
        Self {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        }
    }
}

impl From<&SourceSpan> for SarifRegion {
    fn from(span: &SourceSpan) -> Self {
        Self {
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.end_line,
            end_column: span.end_column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_references_percent_encode_everything_but_unreserved_characters_and_slashes() {
        assert_eq!(uri_reference("lib/main.dart"), "lib/main.dart");
        assert_eq!(uri_reference("lib\\ui\\a_b-c~d.dart"), "lib/ui/a_b-c~d.dart");
        assert_eq!(uri_reference("lib/a b.dart"), "lib/a%20b.dart");
        assert_eq!(uri_reference("lib/a#b?c%d.dart"), "lib/a%23b%3Fc%25d.dart");
        assert_eq!(
            uri_reference("lib/файл.dart"),
            "lib/%D1%84%D0%B0%D0%B9%D0%BB.dart"
        );
        assert_eq!(uri_reference("c:/a.dart"), "c%3A/a.dart");
    }

    #[test]
    fn findings_without_a_span_are_reported_at_the_first_line() {
        let location = SarifLocation::new("lib/a.dart", None);
        let region = location.physical_location.region.expect("region");
        assert_eq!((region.start_line, region.start_column), (1, 1));

        let related = SarifPhysicalLocation::new("lib/b.dart", None);
        assert!(related.region.is_none());
    }
}
