use dartscope_core::{DartUriReferenceKind, DartUriResolution};

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
        // An `export` makes the target part of the layer's public surface, so it crosses the
        // boundary exactly like an `import`. A `part` stays inside its own library.
        if !context.includes_path(&reference.source_path)
            || !matches!(
                reference.kind,
                DartUriReferenceKind::Import | DartUriReferenceKind::Export
            )
            || reference.resolution != DartUriResolution::Resolved
        {
            continue;
        }
        let Some(target_path) = reference.target_path.as_deref() else {
            continue;
        };
        for boundary in &boundaries {
            if !config.path_has_prefix(&reference.source_path, &boundary.source_prefix) {
                continue;
            }
            if let Some(denied_prefix) = denied_prefix(config, boundary, target_path) {
                diagnostics.push(diagnostic(
                    DartLintRuleId::LayerBoundary,
                    severity,
                    format!(
                        "layer `{}` must not {} target `{}` matched by `{}`",
                        boundary.source_prefix,
                        if reference.kind == DartUriReferenceKind::Export {
                            "export"
                        } else {
                            "import"
                        },
                        target_path,
                        denied_prefix
                    ),
                    reference.source_path.clone(),
                    Some(reference.source_span.clone()),
                    vec![target_path.to_string()],
                ));
            }
        }
    }
}

fn denied_prefix<'a>(
    config: &DartLintConfig,
    boundary: &'a DartLayerBoundary,
    target_path: &str,
) -> Option<&'a str> {
    boundary
        .denied_target_prefixes
        .iter()
        .filter(|prefix| config.path_has_prefix(target_path, prefix))
        .map(String::as_str)
        .min()
}
