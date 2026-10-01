//! The per-library GraphQL contract cache and the libraries an update invalidates.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dartscope_core::{
    DartGraphqlContractAnalysis, DartPartLinkAnalysis, DartProjectAnalysis, DartUriGraph,
};

use crate::graphql::{GraphqlContractAnalyzer, sort_contract_analysis};
use crate::namespace::LibraryMembership;
use crate::uri_graph::DartIndexOptions;

use super::libraries::graphql_library_owners;

pub(super) fn build_graphql_contract_cache(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
    uri_graph: Arc<DartUriGraph>,
    part_links: &DartPartLinkAnalysis,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> (
    BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
    DartGraphqlContractAnalysis,
) {
    let analyzer = GraphqlContractAnalyzer::from_analyses(project, options, uri_graph, part_links);
    let mut cache = BTreeMap::new();
    for owner in graphql_library_owners(project, library_paths) {
        let Some(paths) = library_paths.get(&owner) else {
            continue;
        };
        cache.insert(owner, Arc::new(analyzer.analyze_paths(paths)));
    }
    let analysis = aggregate_graphql_contracts(&cache);
    (cache, analysis)
}

/// One side of an update: a project analysis together with the part links computed for it.
#[derive(Clone, Copy)]
pub(super) struct ProjectLinks<'a> {
    pub(super) project: &'a DartProjectAnalysis,
    pub(super) part_links: &'a DartPartLinkAnalysis,
}

/// The libraries whose GraphQL contracts an update has to recompute, judged against both the
/// `old` and the `new` side so that a moved or removed file invalidates the library it left.
pub(super) fn graphql_rebuild_libraries(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    changed_operation_names: &BTreeSet<String>,
    old: ProjectLinks<'_>,
    new: ProjectLinks<'_>,
    global_invalidation: bool,
) -> BTreeSet<String> {
    let new_membership = LibraryMembership::from_part_links(new.part_links);
    if global_invalidation {
        return new
            .project
            .files
            .iter()
            .filter(|file| !file.graphql_operation_uses.is_empty())
            .map(|file| new_membership.owner_of(&file.path).to_string())
            .collect();
    }

    let old_membership = LibraryMembership::from_part_links(old.part_links);
    let mut libraries = BTreeSet::new();
    for path in changed_paths.iter().chain(affected_paths) {
        libraries.insert(old_membership.owner_of(path).to_string());
        libraries.insert(new_membership.owner_of(path).to_string());
    }
    add_graphql_use_libraries(
        old.project,
        &old_membership,
        changed_operation_names,
        &mut libraries,
    );
    add_graphql_use_libraries(
        new.project,
        &new_membership,
        changed_operation_names,
        &mut libraries,
    );
    libraries
}

fn add_graphql_use_libraries(
    project: &DartProjectAnalysis,
    membership: &LibraryMembership,
    names: &BTreeSet<String>,
    libraries: &mut BTreeSet<String>,
) {
    if names.is_empty() {
        return;
    }
    for file in &project.files {
        if file
            .graphql_operation_uses
            .iter()
            .any(|operation_use| names.contains(&operation_use.constant_name))
        {
            libraries.insert(membership.owner_of(&file.path).to_string());
        }
    }
}

pub(super) fn aggregate_graphql_contracts(
    cache: &BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
) -> DartGraphqlContractAnalysis {
    let mut analysis = DartGraphqlContractAnalysis::default();
    for library in cache.values() {
        analysis.bindings.extend(library.bindings.iter().cloned());
        analysis
            .unresolved_uses
            .extend(library.unresolved_uses.iter().cloned());
    }
    sort_contract_analysis(&mut analysis);
    analysis
}
