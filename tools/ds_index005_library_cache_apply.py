#!/usr/bin/env python3
"""Apply the DS-INDEX-005 per-library namespace and GraphQL cache slice."""

from pathlib import Path

ROOT = Path(".")


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one anchor, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")


NAMESPACE = "crates/dartscope-index/src/namespace.rs"
GRAPHQL = "crates/dartscope-index/src/graphql.rs"
INCREMENTAL = "crates/dartscope-index/src/incremental.rs"
TESTS = "crates/dartscope-index/src/tests/incremental.rs"
BASELINE = "crates/dartscope-index/examples/incremental_workspace_baseline.rs"
ROADMAP = "docs/development/dartscope-library-plan.md"
DOC = "docs/development/incremental-index.md"
CHANGELOG = "CHANGELOG.md"

# Namespace resolver: reuse the already-built URI graph and part-link analysis.
replace_once(
    NAMESPACE,
    """use std::collections::{HashMap, HashSet};

use dartscope_core::{
    DartDeclaration, DartFileAnalysis, DartNamespaceCombinatorKind, DartPartLinkStatus,
""",
    """use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dartscope_core::{
    DartDeclaration, DartFileAnalysis, DartNamespaceCombinatorKind, DartPartLinkAnalysis,
    DartPartLinkStatus,
""",
)
replace_once(
    NAMESPACE,
    """use crate::parts::analyze_part_links;
""",
    """use crate::parts::analyze_part_links_with_graph;
""",
)
replace_once(
    NAMESPACE,
    """#[derive(Default)]
struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
}
""",
    """#[derive(Default)]
pub(crate) struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
}
""",
)
replace_once(
    NAMESPACE,
    """impl LibraryMembership {
    fn from_project(project: &DartProjectAnalysis) -> Self {
        let mut membership = Self::default();
        let mut owners_by_part: HashMap<String, Vec<String>> = HashMap::new();
        for link in analyze_part_links(project)
            .links
            .into_iter()
            .filter(|link| link.status == DartPartLinkStatus::Matched)
        {
            let Some(part_path) = link.part_path else {
                continue;
            };
            owners_by_part
                .entry(part_path)
                .or_default()
                .push(link.owner_path);
        }
        for (part_path, mut owners) in owners_by_part {
            owners.sort();
            owners.dedup();
            let [owner_path] = owners.as_slice() else {
                continue;
            };
            membership
                .owner_by_part
                .insert(part_path, owner_path.clone());
        }
        membership
    }

    fn owner_of<'a>(&'a self, path: &'a str) -> &'a str {
        self.owner_by_part
            .get(path)
            .map(String::as_str)
            .unwrap_or(path)
    }

    fn is_part(&self, path: &str) -> bool {
        self.owner_by_part.contains_key(path)
    }

    fn same_library(&self, left: &str, right: &str) -> bool {
        self.owner_of(left) == self.owner_of(right)
    }
}
""",
    """impl LibraryMembership {
    pub(crate) fn from_part_links(analysis: &DartPartLinkAnalysis) -> Self {
        let mut membership = Self::default();
        let mut owners_by_part: HashMap<String, Vec<String>> = HashMap::new();
        for link in analysis
            .links
            .iter()
            .filter(|link| link.status == DartPartLinkStatus::Matched)
        {
            let Some(part_path) = link.part_path.as_ref() else {
                continue;
            };
            owners_by_part
                .entry(part_path.clone())
                .or_default()
                .push(link.owner_path.clone());
        }
        for (part_path, mut owners) in owners_by_part {
            owners.sort();
            owners.dedup();
            let [owner_path] = owners.as_slice() else {
                continue;
            };
            membership
                .owner_by_part
                .insert(part_path, owner_path.clone());
        }
        membership
    }

    pub(crate) fn owner_of<'a>(&'a self, path: &'a str) -> &'a str {
        self.owner_by_part
            .get(path)
            .map(String::as_str)
            .unwrap_or(path)
    }

    pub(crate) fn is_part(&self, path: &str) -> bool {
        self.owner_by_part.contains_key(path)
    }

    pub(crate) fn same_library(&self, left: &str, right: &str) -> bool {
        self.owner_of(left) == self.owner_of(right)
    }
}
""",
)
replace_once(
    NAMESPACE,
    """pub(crate) struct NamespaceResolver<'source, 'options> {
    uri_graph: DartUriGraph,
""",
    """pub(crate) struct NamespaceResolver<'source, 'options> {
    uri_graph: Arc<DartUriGraph>,
""",
)
replace_once(
    NAMESPACE,
    """    pub(crate) fn new(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
    ) -> Self {
        Self {
            uri_graph: build_uri_graph_with_options(project, options),
            library_membership: LibraryMembership::from_project(project),
            files_by_path: project
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect(),
            options,
        }
    }
""",
    """    pub(crate) fn new(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
    ) -> Self {
        let uri_graph = Arc::new(build_uri_graph_with_options(project, options));
        let part_links = analyze_part_links_with_graph(project, &uri_graph);
        Self::from_analyses(project, options, uri_graph, &part_links)
    }

    pub(crate) fn from_analyses(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
        uri_graph: Arc<DartUriGraph>,
        part_links: &DartPartLinkAnalysis,
    ) -> Self {
        Self {
            uri_graph,
            library_membership: LibraryMembership::from_part_links(part_links),
            files_by_path: project
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect(),
            options,
        }
    }
""",
)

# GraphQL analyzer: construct one namespace resolver and operation index, then analyze selected libraries.
replace_once(
    GRAPHQL,
    """use std::collections::HashMap;

use dartscope_core::{
""",
    """use std::collections::HashMap;
use std::sync::Arc;

use dartscope_core::{
""",
)
replace_once(
    GRAPHQL,
    """    DartGraphqlOperationUse, DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason,
    DartGraphqlVariableCompatibility, DartProjectAnalysis, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus, SourceSpan,
""",
    """    DartGraphqlOperationUse, DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason,
    DartGraphqlVariableCompatibility, DartPartLinkAnalysis, DartProjectAnalysis,
    DartSymbolResolutionBasis, DartSymbolResolutionStatus, DartUriGraph, SourceSpan,
""",
)
replace_once(
    GRAPHQL,
    """struct OperationLocation<'a> {
    path: &'a str,
    operation: &'a DartGraphqlOperation,
}

pub fn analyze_graphql_contracts(project: &DartProjectAnalysis) -> DartGraphqlContractAnalysis {
""",
    """struct OperationLocation<'a> {
    path: &'a str,
    operation: &'a DartGraphqlOperation,
}

pub(crate) struct GraphqlContractAnalyzer<'source, 'options> {
    project: &'source DartProjectAnalysis,
    resolver: NamespaceResolver<'source, 'options>,
    operations_by_constant: HashMap<&'source str, Vec<OperationLocation<'source>>>,
    files_by_path: HashMap<&'source str, &'source DartFileAnalysis>,
}

impl<'source, 'options> GraphqlContractAnalyzer<'source, 'options> {
    fn new(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
    ) -> Self {
        Self::with_resolver(project, NamespaceResolver::new(project, options))
    }

    pub(crate) fn from_analyses(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
        uri_graph: Arc<DartUriGraph>,
        part_links: &DartPartLinkAnalysis,
    ) -> Self {
        Self::with_resolver(
            project,
            NamespaceResolver::from_analyses(project, options, uri_graph, part_links),
        )
    }

    fn with_resolver(
        project: &'source DartProjectAnalysis,
        resolver: NamespaceResolver<'source, 'options>,
    ) -> Self {
        Self {
            project,
            resolver,
            operations_by_constant: collect_operations(project),
            files_by_path: project
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect(),
        }
    }

    fn analyze_all(&self) -> DartGraphqlContractAnalysis {
        self.analyze_files(self.project.files.iter())
    }

    pub(crate) fn analyze_paths(&self, paths: &[String]) -> DartGraphqlContractAnalysis {
        self.analyze_files(
            paths
                .iter()
                .filter_map(|path| self.files_by_path.get(path.as_str()).copied()),
        )
    }

    fn analyze_files<I>(&self, files: I) -> DartGraphqlContractAnalysis
    where
        I: IntoIterator<Item = &'source DartFileAnalysis>,
    {
        let mut analysis = DartGraphqlContractAnalysis::default();
        for file in files {
            for operation_use in &file.graphql_operation_uses {
                let candidates = self
                    .operations_by_constant
                    .get(operation_use.constant_name.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                record_use(
                    &self.resolver,
                    &mut analysis,
                    file,
                    operation_use,
                    candidates,
                );
            }
        }
        sort_contract_analysis(&mut analysis);
        analysis
    }
}

pub fn analyze_graphql_contracts(project: &DartProjectAnalysis) -> DartGraphqlContractAnalysis {
""",
)
replace_once(
    GRAPHQL,
    """pub fn analyze_graphql_contracts_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartGraphqlContractAnalysis {
    let resolver = NamespaceResolver::new(project, options);
    let operations_by_constant = collect_operations(project);
    let mut analysis = DartGraphqlContractAnalysis::default();

    for file in &project.files {
        for operation_use in &file.graphql_operation_uses {
            let candidates = operations_by_constant
                .get(operation_use.constant_name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            record_use(&resolver, &mut analysis, file, operation_use, candidates);
        }
    }

    sort_contract_analysis(&mut analysis);
    analysis
}
""",
    """pub fn analyze_graphql_contracts_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartGraphqlContractAnalysis {
    GraphqlContractAnalyzer::new(project, options).analyze_all()
}
""",
)
replace_once(
    GRAPHQL,
    """fn sort_contract_analysis(analysis: &mut DartGraphqlContractAnalysis) {
""",
    """pub(crate) fn sort_contract_analysis(analysis: &mut DartGraphqlContractAnalysis) {
""",
)

# Incremental index fields, counters, initialization, invalidation, and aggregate cache.
replace_once(
    INCREMENTAL,
    """use crate::graphql::analyze_graphql_contracts_with_options;
use crate::parts::analyze_part_links_with_graph;
""",
    """use crate::graphql::{GraphqlContractAnalyzer, sort_contract_analysis};
use crate::namespace::LibraryMembership;
use crate::parts::analyze_part_links_with_graph;
""",
)
replace_once(
    INCREMENTAL,
    """    pub part_link_rebuilds: u64,
    pub graphql_rebuilds: u64,
    pub reference_rebuilds: u64,
""",
    """    pub part_link_rebuilds: u64,
    pub namespace_libraries_rebuilt: u64,
    pub graphql_rebuilds: u64,
    pub graphql_libraries_rebuilt: u64,
    pub reference_rebuilds: u64,
""",
)
replace_once(
    INCREMENTAL,
    """    uri_references_by_path: BTreeMap<String, Arc<Vec<DartUriReference>>>,
    reference_resolutions_by_path: BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
""",
    """    uri_references_by_path: BTreeMap<String, Arc<Vec<DartUriReference>>>,
    library_paths_by_owner: BTreeMap<String, Arc<Vec<String>>>,
    graphql_contracts_by_library: BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
    reference_resolutions_by_path: BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
""",
)
replace_once(
    INCREMENTAL,
    """        let part_links = Arc::new(analyze_part_links_with_graph(&project, &uri_graph));
        let graphql_contracts =
            Arc::new(analyze_graphql_contracts_with_options(&project, &options));
        let (reference_resolutions_by_path, identifier_reference_resolutions) =
""",
    """        let part_links = Arc::new(analyze_part_links_with_graph(&project, &uri_graph));
        let library_paths_by_owner = build_library_path_cache(&project, &part_links);
        let (graphql_contracts_by_library, graphql_contracts) = build_graphql_contract_cache(
            &project,
            &options,
            Arc::clone(&uri_graph),
            &part_links,
            &library_paths_by_owner,
        );
        let graphql_contracts = Arc::new(graphql_contracts);
        let (reference_resolutions_by_path, identifier_reference_resolutions) =
""",
)
replace_once(
    INCREMENTAL,
    """        let initial_uri_files = uri_references_by_path.len() as u64;
        let initial_reference_files = reference_resolutions_by_path.len() as u64;
""",
    """        let initial_uri_files = uri_references_by_path.len() as u64;
        let initial_namespace_libraries = library_paths_by_owner.len() as u64;
        let initial_graphql_libraries = graphql_contracts_by_library.len() as u64;
        let initial_reference_files = reference_resolutions_by_path.len() as u64;
""",
)
replace_once(
    INCREMENTAL,
    """            references_by_path,
            uri_references_by_path,
            reference_resolutions_by_path,
""",
    """            references_by_path,
            uri_references_by_path,
            library_paths_by_owner,
            graphql_contracts_by_library,
            reference_resolutions_by_path,
""",
)
replace_once(
    INCREMENTAL,
    """                part_link_rebuilds: 1,
                graphql_rebuilds: 1,
                reference_rebuilds: 1,
""",
    """                part_link_rebuilds: 1,
                namespace_libraries_rebuilt: initial_namespace_libraries,
                graphql_rebuilds: 1,
                graphql_libraries_rebuilt: initial_graphql_libraries,
                reference_rebuilds: 1,
""",
)
replace_once(
    INCREMENTAL,
    """        let changed_declaration_names =
            changed_top_level_declaration_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
""",
    """        let changed_declaration_names =
            changed_top_level_declaration_names(old_file.as_ref(), Some(&file));
        let changed_graphql_operation_names =
            changed_graphql_operation_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
""",
)
replace_once(
    INCREMENTAL,
    """            changed_declaration_names,
        )
""",
    """            changed_declaration_names,
            changed_graphql_operation_names,
        )
""",
)
replace_once(
    INCREMENTAL,
    """        let changed_declaration_names = changed_top_level_declaration_names(Some(&removed), None);
        self.references_by_path.remove(&path);
""",
    """        let changed_declaration_names = changed_top_level_declaration_names(Some(&removed), None);
        let changed_graphql_operation_names =
            changed_graphql_operation_names(Some(&removed), None);
        self.references_by_path.remove(&path);
""",
)
replace_once(
    INCREMENTAL,
    """            changed_declaration_names,
        )
    }

    /// Inserts or replaces one pubspec analysis.
""",
    """            changed_declaration_names,
            changed_graphql_operation_names,
        )
    }

    /// Inserts or replaces one pubspec analysis.
""",
)

# Every non-file rebuild supplies empty declaration and GraphQL-operation name sets.
target = ROOT / INCREMENTAL
text = target.read_text(encoding="utf-8")
old = """            BTreeSet::new(),
        )
"""
new = """            BTreeSet::new(),
            BTreeSet::new(),
        )
"""
count = text.count(old)
if count != 6:
    raise SystemExit(f"{INCREMENTAL}: expected six non-file rebuild calls, found {count}")
target.write_text(text.replace(old, new), encoding="utf-8")

replace_once(
    INCREMENTAL,
    """        file_set_changed: bool,
        changed_declaration_names: BTreeSet<String>,
    ) -> DartWorkspaceUpdate {
""",
    """        file_set_changed: bool,
        changed_declaration_names: BTreeSet<String>,
        changed_graphql_operation_names: BTreeSet<String>,
    ) -> DartWorkspaceUpdate {
""",
)
replace_once(
    INCREMENTAL,
    """        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links_with_graph(&project, &uri_graph))
        } else {
            Arc::clone(&old.part_links)
        };
        let mut affected_paths: BTreeSet<_> = affected_paths(
""",
    """        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links_with_graph(&project, &uri_graph))
        } else {
            Arc::clone(&old.part_links)
        };
        if plan.part_links || file_set_changed {
            self.counters.namespace_libraries_rebuilt += refresh_library_path_cache(
                &project,
                &part_links,
                &mut self.library_paths_by_owner,
            );
        }
        let mut affected_paths: BTreeSet<_> = affected_paths(
""",
)
replace_once(
    INCREMENTAL,
    """        let graphql_contracts = if plan.graphql_contracts {
            self.counters.graphql_rebuilds += 1;
            Arc::new(analyze_graphql_contracts_with_options(
                &project,
                &self.options,
            ))
        } else {
            Arc::clone(&old.graphql_contracts)
        };
""",
    """        let graphql_contracts = if plan.graphql_contracts {
            self.counters.graphql_rebuilds += 1;
            let active_libraries = graphql_library_owners(&project, &self.library_paths_by_owner);
            let rebuild_libraries = graphql_rebuild_libraries(
                &changed_paths,
                &affected_paths,
                &changed_graphql_operation_names,
                &old.project,
                &old.part_links,
                &project,
                &part_links,
                global_invalidation,
            );
            let mut rebuilt_libraries = 0_u64;
            if global_invalidation {
                self.graphql_contracts_by_library.clear();
            } else {
                let before = self.graphql_contracts_by_library.len();
                self.graphql_contracts_by_library
                    .retain(|owner, _| active_libraries.contains(owner));
                rebuilt_libraries +=
                    (before - self.graphql_contracts_by_library.len()) as u64;
            }
            let analyzer = GraphqlContractAnalyzer::from_analyses(
                &project,
                &self.options,
                Arc::clone(&uri_graph),
                &part_links,
            );
            for owner in rebuild_libraries {
                if !active_libraries.contains(&owner) {
                    self.graphql_contracts_by_library.remove(&owner);
                    continue;
                }
                let Some(paths) = self.library_paths_by_owner.get(&owner) else {
                    continue;
                };
                self.graphql_contracts_by_library
                    .insert(owner, Arc::new(analyzer.analyze_paths(paths)));
                rebuilt_libraries += 1;
            }
            for owner in active_libraries {
                if self.graphql_contracts_by_library.contains_key(&owner) {
                    continue;
                }
                let Some(paths) = self.library_paths_by_owner.get(&owner) else {
                    continue;
                };
                self.graphql_contracts_by_library
                    .insert(owner, Arc::new(analyzer.analyze_paths(paths)));
                rebuilt_libraries += 1;
            }
            self.counters.graphql_libraries_rebuilt += rebuilt_libraries;
            Arc::new(aggregate_graphql_contracts(
                &self.graphql_contracts_by_library,
            ))
        } else {
            Arc::clone(&old.graphql_contracts)
        };
""",
)

replace_once(
    INCREMENTAL,
    """fn top_level_declaration_facts(
""",
    """fn changed_graphql_operation_names(
    old: Option<&DartFileAnalysis>,
    new: Option<&DartFileAnalysis>,
) -> BTreeSet<String> {
    let old_operations = old
        .map(|file| file.graphql_operations.as_slice())
        .unwrap_or_default();
    let new_operations = new
        .map(|file| file.graphql_operations.as_slice())
        .unwrap_or_default();
    if old_operations == new_operations {
        return BTreeSet::new();
    }
    old_operations
        .iter()
        .chain(new_operations)
        .map(|operation| operation.constant_name.clone())
        .collect()
}

fn top_level_declaration_facts(
""",
)
replace_once(
    INCREMENTAL,
    """fn build_uri_reference_cache(
""",
    """fn grouped_library_paths(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
) -> BTreeMap<String, Vec<String>> {
    let membership = LibraryMembership::from_part_links(part_links);
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &project.files {
        grouped
            .entry(membership.owner_of(&file.path).to_string())
            .or_default()
            .push(file.path.clone());
    }
    for paths in grouped.values_mut() {
        paths.sort();
        paths.dedup();
    }
    grouped
}

fn build_library_path_cache(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
) -> BTreeMap<String, Arc<Vec<String>>> {
    grouped_library_paths(project, part_links)
        .into_iter()
        .map(|(owner, paths)| (owner, Arc::new(paths)))
        .collect()
}

fn refresh_library_path_cache(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
    cache: &mut BTreeMap<String, Arc<Vec<String>>>,
) -> u64 {
    let grouped = grouped_library_paths(project, part_links);
    let before = cache.len();
    cache.retain(|owner, _| grouped.contains_key(owner));
    let mut rebuilt = (before - cache.len()) as u64;
    for (owner, paths) in grouped {
        if cache
            .get(&owner)
            .is_some_and(|existing| existing.as_ref() == &paths)
        {
            continue;
        }
        cache.insert(owner, Arc::new(paths));
        rebuilt += 1;
    }
    rebuilt
}

fn graphql_library_owners(
    project: &DartProjectAnalysis,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> BTreeSet<String> {
    let use_paths: BTreeSet<_> = project
        .files
        .iter()
        .filter(|file| !file.graphql_operation_uses.is_empty())
        .map(|file| file.path.as_str())
        .collect();
    library_paths
        .iter()
        .filter(|(_, paths)| paths.iter().any(|path| use_paths.contains(path.as_str())))
        .map(|(owner, _)| owner.clone())
        .collect()
}

fn build_graphql_contract_cache(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
    uri_graph: Arc<DartUriGraph>,
    part_links: &DartPartLinkAnalysis,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> (
    BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
    DartGraphqlContractAnalysis,
) {
    let analyzer =
        GraphqlContractAnalyzer::from_analyses(project, options, uri_graph, part_links);
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

#[allow(clippy::too_many_arguments)]
fn graphql_rebuild_libraries(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    changed_operation_names: &BTreeSet<String>,
    old_project: &DartProjectAnalysis,
    old_part_links: &DartPartLinkAnalysis,
    new_project: &DartProjectAnalysis,
    new_part_links: &DartPartLinkAnalysis,
    global_invalidation: bool,
) -> BTreeSet<String> {
    let new_membership = LibraryMembership::from_part_links(new_part_links);
    if global_invalidation {
        return new_project
            .files
            .iter()
            .filter(|file| !file.graphql_operation_uses.is_empty())
            .map(|file| new_membership.owner_of(&file.path).to_string())
            .collect();
    }

    let old_membership = LibraryMembership::from_part_links(old_part_links);
    let mut libraries = BTreeSet::new();
    for path in changed_paths.iter().chain(affected_paths) {
        libraries.insert(old_membership.owner_of(path).to_string());
        libraries.insert(new_membership.owner_of(path).to_string());
    }
    add_graphql_use_libraries(
        old_project,
        &old_membership,
        changed_operation_names,
        &mut libraries,
    );
    add_graphql_use_libraries(
        new_project,
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

fn aggregate_graphql_contracts(
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

fn build_uri_reference_cache(
""",
)

# Tests: independent libraries, unrelated NotVisible evidence, and sibling parts.
replace_once(
    TESTS,
    """#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
""",
    """#[test]
fn per_library_graphql_cache_rebuilds_only_the_affected_use_library() {
    let initial = reference_project(&[
        (
            "lib/a_api.dart",
            "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
        ),
        (
            "lib/a_client.dart",
            "import 'a_api.dart';\nvoid loadA() { client.query(QueryOptions(document: gql(viewerQuery))); }\n",
        ),
        (
            "lib/b_api.dart",
            "const accountQuery = r'''query Account { account { id } }''';\n",
        ),
        (
            "lib/b_client.dart",
            "import 'b_api.dart';\nvoid loadB() { client.query(QueryOptions(document: gql(accountQuery))); }\n",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    assert_eq!(before.graphql_libraries_rebuilt, 2);

    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new(
            "lib/a_api.dart",
            "const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';\n",
        ),
    ));

    assert_eq!(update.affected_paths, vec!["lib/a_api.dart", "lib/a_client.dart"]);
    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/a_api.dart",
            "const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';\n",
        ),
        (
            "lib/a_client.dart",
            "import 'a_api.dart';\nvoid loadA() { client.query(QueryOptions(document: gql(viewerQuery))); }\n",
        ),
        (
            "lib/b_api.dart",
            "const accountQuery = r'''query Account { account { id } }''';\n",
        ),
        (
            "lib/b_client.dart",
            "import 'b_api.dart';\nvoid loadB() { client.query(QueryOptions(document: gql(accountQuery))); }\n",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
}

#[test]
fn graphql_not_visible_evidence_rebuilds_without_a_uri_edge() {
    let initial = reference_project(&[
        (
            "lib/use.dart",
            "void load() { client.query(QueryOptions(document: gql(hiddenQuery))); }\n",
        ),
        (
            "lib/hidden.dart",
            "const hiddenQuery = r'''query Hidden { hidden { id } }''';\n",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    let unresolved = &index.snapshot().graphql_contracts().unresolved_uses[0];
    assert_eq!(
        unresolved.reason,
        DartGraphqlUnresolvedReason::NotVisibleDeclaration
    );
    assert_eq!(unresolved.candidate_paths, vec!["lib/hidden.dart"]);

    index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
        "lib/hidden.dart",
        "const renamedQuery = r'''query Hidden { hidden { id } }''';\n",
    )));

    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/use.dart",
            "void load() { client.query(QueryOptions(document: gql(hiddenQuery))); }\n",
        ),
        (
            "lib/hidden.dart",
            "const renamedQuery = r'''query Hidden { hidden { id } }''';\n",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    let unresolved = &index.snapshot().graphql_contracts().unresolved_uses[0];
    assert_eq!(unresolved.reason, DartGraphqlUnresolvedReason::MissingDeclaration);
    assert!(unresolved.candidate_paths.is_empty());
}

#[test]
fn graphql_cache_groups_operation_uses_by_part_library() {
    let initial = reference_project(&[
        (
            "lib/owner.dart",
            "part 'operation.dart';\npart 'use.dart';\n",
        ),
        (
            "lib/operation.dart",
            "part of 'owner.dart';\nconst viewerQuery = r'''query Viewer { viewer { id } }''';\n",
        ),
        (
            "lib/use.dart",
            "part of 'owner.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }\n",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    assert_eq!(before.graphql_libraries_rebuilt, 1);
    assert_eq!(index.snapshot().graphql_contracts().bindings.len(), 1);

    index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
        "lib/operation.dart",
        "part of 'owner.dart';\nconst viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';\n",
    )));

    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/owner.dart",
            "part 'operation.dart';\npart 'use.dart';\n",
        ),
        (
            "lib/operation.dart",
            "part of 'owner.dart';\nconst viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';\n",
        ),
        (
            "lib/use.dart",
            "part of 'owner.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }\n",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
}

#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
""",
)
replace_once(
    TESTS,
    """    assert_eq!(after.part_link_rebuilds, before.part_link_rebuilds);
    assert_eq!(after.graphql_rebuilds, before.graphql_rebuilds);
""",
    """    assert_eq!(after.part_link_rebuilds, before.part_link_rebuilds);
    assert_eq!(
        after.namespace_libraries_rebuilt,
        before.namespace_libraries_rebuilt
    );
    assert_eq!(after.graphql_rebuilds, before.graphql_rebuilds);
    assert_eq!(
        after.graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt
    );
""",
)

replace_once(
    BASELINE,
    """        assert_eq!(counters.uri_files_rebuilt, file_count as u64);
        assert_eq!(counters.reference_files_rebuilt, 0);
""",
    """        assert_eq!(counters.uri_files_rebuilt, file_count as u64);
        assert_eq!(counters.namespace_libraries_rebuilt, file_count as u64);
        assert_eq!(counters.graphql_libraries_rebuilt, 0);
        assert_eq!(counters.reference_files_rebuilt, 0);
""",
)

replace_once(
    ROADMAP,
    """11. **P1 fixed:** the first part-component helper echoed a changed metadata path into public
    `affected_paths`; it now returns only newly reached Dart owner/part paths.

Remaining work:

1. Add per-library namespace and GraphQL binding caches while preserving the public aggregate snapshot
   models and stateless output equivalence.
2. Feed the same affected-library evidence into lint contexts without introducing an index/lint
   dependency cycle.
3. Add memory/update-time baselines for the per-library cache implementation.
""",
    """11. **P1 fixed:** the first part-component helper echoed a changed metadata path into public
    `affected_paths`; it now returns only newly reached Dart owner/part paths.
12. Added retained per-library namespace-membership and GraphQL-binding caches. GraphQL operation
    changes rebuild only libraries with affected uses, including unrelated `NotVisible` evidence and
    sibling parts, while the public aggregate snapshot remains unchanged.

Remaining work:

1. Add persistent per-library import/export dependency fingerprints for lint-context reuse beyond the
   completed membership and GraphQL binding caches.
2. Feed the same affected-library evidence into lint contexts without introducing an index/lint
   dependency cycle.
3. Add memory/update-time baselines for the per-library cache implementation.
""",
)
replace_once(
    DOC,
    """`DartWorkspaceIndexCounters` records generations, aggregate rebuilds, and the exact number of URI
source files and identifier-reference source files recomputed. Unaffected per-file cache entries remain
shared internally. These are semantic operation counters rather than elapsed-time assertions, so they
are deterministic across Linux, Windows, and differently loaded runners.
""",
    """`DartWorkspaceIndexCounters` records generations, aggregate rebuilds, the exact number of URI and
identifier-reference source files recomputed, and the number of namespace-membership and GraphQL-use
libraries refreshed. Unaffected per-file and per-library `Arc` cache entries remain shared internally.
These are semantic operation counters rather than elapsed-time assertions, so they are deterministic
across Linux, Windows, and differently loaded runners.
""",
)
replace_once(
    DOC,
    """URI references and identifier-reference resolutions now use per-source-file caches while public
snapshots retain the same aggregate models. A later DS-INDEX-005 slice will add per-library namespace and
GraphQL binding caches, then expose the same invalidation evidence to lint contexts. The public stateful
API and existing stateless APIs remain stable while that internal granularity improves.
""",
    """URI references and identifier-reference resolutions use per-source-file caches. Library membership
and GraphQL bindings use retained per-library caches, while public snapshots retain the same aggregate
models. A later DS-INDEX-005 slice will persist import/export dependency fingerprints and expose the same
affected-library evidence to lint contexts. The public stateful API and existing stateless APIs remain
stable while that internal granularity improves.
""",
)
replace_once(
    CHANGELOG,
    """- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
  visibility changes without leaking non-Dart metadata paths into `affected_paths`.
""",
    """- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
  visibility changes without leaking non-Dart metadata paths into `affected_paths`.
- Retained per-library namespace-membership and GraphQL-binding caches rebuild only affected GraphQL-use
  libraries while preserving the existing aggregate snapshot contract.
""",
)

print("DS-INDEX-005 per-library cache slice applied")
