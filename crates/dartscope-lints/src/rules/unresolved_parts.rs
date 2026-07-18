use dartscope_core::DartPartLinkStatus;

use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLintConfig, DartLintDiagnostic, DartLintRuleId};

pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    let Some(part_links) = context.part_links() else {
        return;
    };
    let severity = config.severity(DartLintRuleId::UnresolvedPart);
    for link in &part_links.links {
        if !context.includes_path(&link.owner_path) || link.status == DartPartLinkStatus::Matched {
            continue;
        }
        let related_paths = link.part_path.clone().into_iter().collect();
        diagnostics.push(diagnostic(
            DartLintRuleId::UnresolvedPart,
            severity,
            format!(
                "part `{}` has unresolved relationship status `{}`",
                link.part_uri,
                status_name(link.status)
            ),
            link.owner_path.clone(),
            Some(link.part_span.clone()),
            related_paths,
        ));
    }
}

fn status_name(status: DartPartLinkStatus) -> &'static str {
    match status {
        DartPartLinkStatus::Matched => "matched",
        DartPartLinkStatus::MissingTarget => "missing_target",
        DartPartLinkStatus::UnresolvedTarget => "unresolved_target",
        DartPartLinkStatus::MissingPartOf => "missing_part_of",
        DartPartLinkStatus::DifferentLibrary => "different_library",
    }
}
