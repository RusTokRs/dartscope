use dartscope_core::{DartPartLinkAnalysis, DartProjectAnalysis, DartUriGraph};
use dartscope_index::{analyze_part_links, build_uri_graph};

use crate::DartLintRuleId;

pub(crate) struct RuleContext<'a> {
    pub(crate) project: &'a DartProjectAnalysis,
    pub(crate) uri_graph: Option<DartUriGraph>,
    pub(crate) part_links: Option<DartPartLinkAnalysis>,
}

impl<'a> RuleContext<'a> {
    pub(crate) fn new(project: &'a DartProjectAnalysis, enabled: &[DartLintRuleId]) -> Self {
        let needs_uri_graph = enabled.iter().any(|rule| {
            matches!(
                rule,
                DartLintRuleId::LayerBoundary | DartLintRuleId::OrphanFile
            )
        });
        let needs_part_links = enabled.contains(&DartLintRuleId::UnresolvedPart);
        Self {
            project,
            uri_graph: needs_uri_graph.then(|| build_uri_graph(project)),
            part_links: needs_part_links.then(|| analyze_part_links(project)),
        }
    }
}
