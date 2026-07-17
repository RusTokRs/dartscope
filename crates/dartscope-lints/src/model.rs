use dartscope_core::{DiagnosticSeverity, SourceSpan};
use serde::{Deserialize, Serialize};

use crate::DartLintRuleId;

/// One deterministic lint finding.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLintDiagnostic {
    pub rule_id: DartLintRuleId,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_paths: Vec<String>,
}

/// Aggregate counts for one lint run.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLintSummary {
    pub enabled_rules: usize,
    pub diagnostics: usize,
    pub info: usize,
    pub warnings: usize,
    pub errors: usize,
}

/// Complete deterministic output of the lint engine.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLintAnalysis {
    pub diagnostics: Vec<DartLintDiagnostic>,
    pub summary: DartLintSummary,
}
