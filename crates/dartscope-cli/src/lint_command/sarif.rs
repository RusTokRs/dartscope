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
            locations: vec![SarifLocation::new(&diagnostic.path, diagnostic.span.as_ref())],
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
            physical_location: SarifPhysicalLocation::new(path, span),
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
    fn new(path: &str, span: Option<&SourceSpan>) -> Self {
        Self {
            artifact_location: SarifArtifactLocation {
                uri: path.replace('\\', "/"),
            },
            region: span.map(SarifRegion::from),
        }
    }
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
