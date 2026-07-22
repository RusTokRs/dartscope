use std::borrow::Cow;
use std::collections::BTreeSet;

use dartscope_core::{
    DartPartLinkAnalysis, DartProjectAnalysis, DartUriGraph, normalize_path,
};
use dartscope_index::{DartWorkspaceSnapshot, analyze_part_links, build_uri_graph};

use crate::DartLintRuleId;

pub(crate) struct RuleContext<'a> {
    pub(crate) project: &'a DartProjectAnalysis,
    uri_graph: Option<Cow<'a, DartUriGraph>>,
    part_links: Option<Cow<'a, DartPartLinkAnalysis>>,
    included_paths: Option<Cow<'a, BTreeSet<String>>>,
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
            included_paths: normalize_included_paths(included_paths),
        }
    }

    pub(crate) fn includes_path(&self, path: &str) -> bool {
        self.included_paths
            .as_deref()
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

fn normalize_included_paths<'a>(
    included_paths: Option<&'a BTreeSet<String>>,
) -> Option<Cow<'a, BTreeSet<String>>> {
    included_paths.map(|paths| {
        if paths.iter().all(|path| !path.contains('\\')) {
            Cow::Borrowed(paths)
        } else {
            Cow::Owned(paths.iter().cloned().map(normalize_path).collect())
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::{DartFileAnalysis, DartProjectAnalysis, DartProjectSummary};
    use dartscope_index::DartWorkspaceIndex;

    #[test]
    fn lint_workspace_paths_normalizes_inputs() {
        let project = DartProjectAnalysis {
            root: ".".to_string(),
            files: vec![DartFileAnalysis::empty("lib/main.dart")],
            pubspecs: Vec::new(),
            package_configs: Vec::new(),
            summary: DartProjectSummary {
                dart_files: 1,
                ..DartProjectSummary::default()
            },
            diagnostics: Vec::new(),
        };
        let index = DartWorkspaceIndex::from_project(project);
        let snapshot = index.snapshot();
        let included_paths = BTreeSet::from(["lib\\main.dart".to_string()]);
        let context = RuleContext::from_snapshot(
            snapshot.as_ref(),
            &[DartLintRuleId::NamingConvention],
            Some(&included_paths),
        );

        assert!(context.includes_path("lib/main.dart"));
        assert!(!context.includes_path("lib/other.dart"));
    }

    #[test]
    fn normalized_path_filters_are_borrowed_without_copying() {
        let included_paths = BTreeSet::from(["lib/main.dart".to_string()]);
        let normalized = normalize_included_paths(Some(&included_paths));

        assert!(matches!(normalized, Some(Cow::Borrowed(_))));
    }
}
