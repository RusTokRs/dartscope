use std::borrow::Cow;
use std::collections::BTreeSet;

use dartscope_core::{DartPartLinkAnalysis, DartProjectAnalysis, DartUriGraph};
use dartscope_index::{DartWorkspaceSnapshot, analyze_part_links, build_uri_graph};

use crate::DartLintRuleId;

pub(crate) struct RuleContext<'a> {
    pub(crate) project: &'a DartProjectAnalysis,
    uri_graph: Option<Cow<'a, DartUriGraph>>,
    part_links: Option<Cow<'a, DartPartLinkAnalysis>>,
    included_paths: Option<&'a BTreeSet<String>>,
}

impl<'a> RuleContext<'a> {
    pub(crate) fn new(project: &'a DartProjectAnalysis, enabled: &[DartLintRuleId]) -> Self {
        let (needs_uri_graph, needs_part_links) = requirements(enabled);
        Self {
            project,
            uri_graph: needs_uri_graph.then(|| Cow::Owned(build_uri_graph(project))),
            part_links: needs_part_links.then(|| Cow::Owned(analyze_part_links(project))),
            included_paths: None,
        }
    }

    pub(crate) fn from_snapshot(
        snapshot: &'a DartWorkspaceSnapshot,
        enabled: &[DartLintRuleId],
        included_paths: Option<&'a BTreeSet<String>>,
    ) -> Self {
        let (needs_uri_graph, needs_part_links) = requirements(enabled);
        Self {
            project: snapshot.project(),
            uri_graph: needs_uri_graph.then(|| Cow::Borrowed(snapshot.uri_graph())),
            part_links: needs_part_links.then(|| Cow::Borrowed(snapshot.part_links())),
            included_paths,
        }
    }

    pub(crate) fn includes_path(&self, path: &str) -> bool {
        self.included_paths
            .map(|included| included.contains(path))
            .unwrap_or(true)
    }

    pub(crate) fn uri_graph(&self) -> Option<&DartUriGraph> {
        self.uri_graph.as_deref()
    }

    pub(crate) fn part_links(&self) -> Option<&DartPartLinkAnalysis> {
        self.part_links.as_deref()
    }
}

fn requirements(enabled: &[DartLintRuleId]) -> (bool, bool) {
    let needs_uri_graph = enabled.iter().any(|rule| {
        matches!(
            rule,
            DartLintRuleId::LayerBoundary | DartLintRuleId::OrphanFile
        )
    });
    let needs_part_links = enabled.contains(&DartLintRuleId::UnresolvedPart);
    (needs_uri_graph, needs_part_links)
}
