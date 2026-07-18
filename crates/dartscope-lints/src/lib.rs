//! Optional deterministic lint rules over normalized DartScope analysis.
//!
//! This crate consumes `dartscope-core` and `dartscope-index` facts. It performs no source parsing
//! and no filesystem I/O.

mod config;
mod context;
mod engine;
mod incremental;
mod model;
mod rules;

pub use config::{
    DartForbiddenImportPattern, DartImportPatternKind, DartLayerBoundary, DartLintConfig,
    DartLintRuleId, DartLintSeverityOverride, DartNamingRuleConfig, DartOrphanFileRuleConfig,
};
pub use engine::{lint_project, lint_workspace_snapshot};
pub use incremental::{
    DartIncrementalLintCache, DartIncrementalLintCounters, DartIncrementalLintUpdate,
};
pub use model::{DartLintAnalysis, DartLintDiagnostic, DartLintSummary};
