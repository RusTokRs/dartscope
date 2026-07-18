use dartscope_core::{DartUriReferenceKind, DartUriResolution, normalize_path};

use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLayerBoundary, DartLintConfig, DartLintDiagnostic, DartLintRuleId};

pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    let Some(uri_graph) = context.uri_graph() else {
        return;
    };
    let mut boundaries = config.layer_boundaries.clone();
    boundaries.sort_by(|left, right| {
        (&left.source_prefix, &left.denied_target_prefixes)
            .cmp(&(&right.source_prefix, &right.denied_target_prefixes))
    });
    let severity = config.severity(DartLintRuleId::LayerBoundary);

    for reference in &uri_graph.references {
        if !context.includes_path(&reference.source_path)
            || reference.kind != DartUriReferenceKind::Import
            || reference.resolution != DartUriResolution::Resolved
        {
            continue;
        }
        let Some(target_path) = reference.target_path.as_deref() else {
            continue;
        };
        for boundary in &boundaries {
            if !reference
                .source_path
                .starts_with(&normalize_path(boundary.source_prefix.clone()))
            {
                continue;
            }
            if let Some(denied_prefix) = denied_prefix(boundary, target_path) {
                diagnostics.push(diagnostic(
                    DartLintRuleId::LayerBoundary,
                    severity,
                    format!(
                        "layer `{}` must not import target `{}` matched by `{}`",
                        boundary.source_prefix, target_path, denied_prefix
                    ),
                    reference.source_path.clone(),
                    Some(reference.source_span.clone()),
                    vec![target_path.to_string()],
                ));
            }
        }
    }
}

fn denied_prefix<'a>(boundary: &'a DartLayerBoundary, target_path: &str) -> Option<&'a str> {
    boundary
        .denied_target_prefixes
        .iter()
        .map(|prefix| (prefix, normalize_path(prefix.clone())))
        .filter(|(_, normalized)| target_path.starts_with(normalized))
        .map(|(prefix, _)| prefix.as_str())
        .min()
}
