pub(crate) mod forbidden_imports;
pub(crate) mod layer_boundaries;
pub(crate) mod naming;
pub(crate) mod orphan_files;
pub(crate) mod unresolved_parts;

use dartscope_core::{DiagnosticSeverity, SourceSpan};

use crate::{DartLintDiagnostic, DartLintRuleId};

pub(crate) fn diagnostic(
    rule_id: DartLintRuleId,
    severity: DiagnosticSeverity,
    message: impl Into<String>,
    path: impl Into<String>,
    span: Option<SourceSpan>,
    related_paths: Vec<String>,
) -> DartLintDiagnostic {
    DartLintDiagnostic {
        rule_id,
        severity,
        message: message.into(),
        path: path.into(),
        span,
        related_paths,
    }
}
