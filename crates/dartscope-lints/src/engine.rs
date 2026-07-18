use std::collections::BTreeSet;

use dartscope_core::DiagnosticSeverity;
use dartscope_index::DartWorkspaceSnapshot;

use crate::context::RuleContext;
use crate::model::{DartLintAnalysis, DartLintDiagnostic, DartLintSummary};
use crate::rules;
use crate::{DartLintConfig, DartLintRuleId};

/// Runs enabled rules over normalized project analysis.
pub fn lint_project(
    project: &dartscope_core::DartProjectAnalysis,
    config: &DartLintConfig,
) -> DartLintAnalysis {
    let enabled = config.enabled_rule_ids();
    let context = RuleContext::new(project, &enabled);
    lint_with_context(&context, config, &enabled)
}

/// Runs enabled rules over one immutable workspace snapshot without rebuilding index products.
pub fn lint_workspace_snapshot(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
) -> DartLintAnalysis {
    let enabled = config.enabled_rule_ids();
    lint_workspace_rules(snapshot, config, &enabled)
}

pub(crate) fn lint_workspace_rules(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
) -> DartLintAnalysis {
    let context = RuleContext::from_snapshot(snapshot, enabled, None);
    lint_with_context(&context, config, enabled)
}

pub(crate) fn lint_workspace_paths(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
    included_paths: &BTreeSet<String>,
) -> DartLintAnalysis {
    let context = RuleContext::from_snapshot(snapshot, enabled, Some(included_paths));
    lint_with_context(&context, config, enabled)
}

fn lint_with_context(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
) -> DartLintAnalysis {
    let mut diagnostics = Vec::new();

    for rule_id in enabled {
        match rule_id {
            DartLintRuleId::ForbiddenImport => {
                rules::forbidden_imports::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::LayerBoundary => {
                rules::layer_boundaries::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::NamingConvention => {
                rules::naming::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::UnresolvedPart => {
                rules::unresolved_parts::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::OrphanFile => {
                rules::orphan_files::run(context, config, &mut diagnostics)
            }
        }
    }

    analysis_from_diagnostics(enabled.len(), diagnostics)
}

pub(crate) fn analysis_from_diagnostics(
    enabled_rules: usize,
    mut diagnostics: Vec<DartLintDiagnostic>,
) -> DartLintAnalysis {
    sort_and_deduplicate(&mut diagnostics);
    let summary = summarize(enabled_rules, &diagnostics);
    DartLintAnalysis {
        diagnostics,
        summary,
    }
}

fn sort_and_deduplicate(diagnostics: &mut Vec<DartLintDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        (
            &left.path,
            left.span.as_ref().map(|span| span.byte_start),
            left.span.as_ref().map(|span| span.byte_end),
            left.rule_id,
            &left.message,
            &left.related_paths,
        )
            .cmp(&(
                &right.path,
                right.span.as_ref().map(|span| span.byte_start),
                right.span.as_ref().map(|span| span.byte_end),
                right.rule_id,
                &right.message,
                &right.related_paths,
            ))
    });
    diagnostics.dedup();
}

fn summarize(enabled_rules: usize, diagnostics: &[DartLintDiagnostic]) -> DartLintSummary {
    let mut summary = DartLintSummary {
        enabled_rules,
        diagnostics: diagnostics.len(),
        ..DartLintSummary::default()
    };
    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Info => summary.info += 1,
            DiagnosticSeverity::Warning => summary.warnings += 1,
            DiagnosticSeverity::Error => summary.errors += 1,
        }
    }
    summary
}
