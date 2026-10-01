//! One incremental rebuild: which subsystems are refreshed, from which caches, in which order.
//!
//! A rebuild walks the derived products of a snapshot in dependency order: project, URI graph,
//! part links, library caches, GraphQL contracts, identifier-reference resolutions. Each `refresh_*`
//! phase either recomputes its product from the caches the index owns or reuses the `Arc` of the
//! previous snapshot, as decided by the `RebuildPlan`.

use std::collections::BTreeSet;
use std::sync::Arc;

use dartscope_core::{
    DartGraphqlContractAnalysis, DartIdentifierReferenceResolutionAnalysis, DartPartLinkAnalysis,
    DartProjectAnalysis, DartUriGraph,
};

use crate::graphql::GraphqlContractAnalyzer;
use crate::parts::analyze_part_links_with_graph;
use crate::references::resolve_identifier_references_with_options;
use crate::uri_graph::UriGraphBuilder;

use super::caches::{
    aggregate_reference_resolutions, aggregate_uri_graph, reference_rebuild_paths, uri_rebuild_paths,
};
use super::graphql_cache::{ProjectLinks, aggregate_graphql_contracts, graphql_rebuild_libraries};
use super::libraries::{
    affected_library_owners, aggregate_library_dependency_fingerprints, graphql_library_owners,
    library_related_paths, refresh_library_dependency_fingerprint_cache, refresh_library_path_cache,
};
use super::plan::{RebuildPlan, affected_paths, reference_sources_for_declaration_names};
use super::project::{aggregate_bindings, aggregate_references, build_project};
use super::{
    DartLibraryDependencyFingerprint, DartWorkspaceIndex, DartWorkspaceSnapshot,
    DartWorkspaceUpdate,
};

/// What the mutation that requested a rebuild established about the update.
struct RebuildTrigger<'a> {
    plan: RebuildPlan,
    /// The generation the rebuild starts from; unchanged products are shared with it.
    old: &'a DartWorkspaceSnapshot,
    changed_paths: BTreeSet<String>,
    changed_declaration_names: BTreeSet<String>,
    changed_graphql_operation_names: BTreeSet<String>,
    global_invalidation: bool,
    file_set_changed: bool,
}

impl DartWorkspaceIndex {
    pub(super) fn rebuild(
        &mut self,
        plan: RebuildPlan,
        changed_paths: BTreeSet<String>,
        global_invalidation: bool,
        file_set_changed: bool,
        changed_declaration_names: BTreeSet<String>,
        changed_graphql_operation_names: BTreeSet<String>,
    ) -> DartWorkspaceUpdate {
        debug_assert!(plan.public().any());
        let old = Arc::clone(&self.snapshot);
        let trigger = RebuildTrigger {
            plan,
            old: &old,
            changed_paths,
            changed_declaration_names,
            changed_graphql_operation_names,
            global_invalidation,
            file_set_changed,
        };

        let project = self.refresh_project(&trigger);
        let uri_graph = self.refresh_uri_graph(&trigger, &project);
        let part_links = self.refresh_part_links(&trigger, &project, &uri_graph);
        let library_dependency_fingerprints =
            self.refresh_library_caches(&trigger, &project, &uri_graph, &part_links);
        let affected_paths = self.collect_affected_paths(&trigger, &project, &uri_graph, &part_links);
        let affected_libraries = affected_library_owners(
            &trigger.changed_paths,
            &affected_paths,
            &old.project,
            &old.part_links,
            &project,
            &part_links,
        );
        let graphql_contracts = self.refresh_graphql_contracts(
            &trigger,
            &project,
            &uri_graph,
            &part_links,
            &affected_paths,
        );
        let identifier_reference_resolutions =
            self.refresh_reference_resolutions(&trigger, &project, &affected_paths);

        let identifier_references = Arc::new(aggregate_references(&self.references_by_path));
        let lexical_bindings = Arc::new(aggregate_bindings(&self.bindings_by_path));

        self.counters.generations += 1;
        let generation = old.generation + 1;
        self.snapshot = Arc::new(DartWorkspaceSnapshot {
            generation,
            project,
            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
            identifier_reference_resolutions,
            identifier_references,
            lexical_bindings,
            options: self.options.clone(),
        });

        DartWorkspaceUpdate {
            generation,
            changed_paths: trigger.changed_paths.into_iter().collect(),
            affected_paths,
            affected_libraries,
            rebuilt: trigger.plan.public(),
        }
    }

    fn refresh_project(&mut self, trigger: &RebuildTrigger<'_>) -> Arc<DartProjectAnalysis> {
        if !trigger.plan.project {
            return Arc::clone(&trigger.old.project);
        }
        self.counters.project_rebuilds += 1;
        Arc::new(build_project(
            &self.root,
            &self.files,
            &self.pubspecs,
            &self.package_configs,
            &self.project_diagnostics,
        ))
    }

    /// Re-extracts the URI references of the files that may have changed and aggregates the graph.
    fn refresh_uri_graph(
        &mut self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
    ) -> Arc<DartUriGraph> {
        if !trigger.plan.uri_graph {
            return Arc::clone(&trigger.old.uri_graph);
        }
        self.counters.uri_graph_rebuilds += 1;
        let rebuild_paths = uri_rebuild_paths(
            &trigger.changed_paths,
            &trigger.old.uri_graph,
            project,
            trigger.global_invalidation,
            trigger.file_set_changed,
        );
        let options = self.options.clone();
        let builder = UriGraphBuilder::new(project, &options);
        if trigger.global_invalidation {
            self.uri_references_by_path.clear();
        }
        let files = &self.files;
        self.uri_references_by_path
            .retain(|path, _| files.contains_key(path));
        let mut rebuilt_files = 0_u64;
        for path in rebuild_paths {
            let Some(file) = self.files.get(&path) else {
                continue;
            };
            self.uri_references_by_path
                .insert(path, Arc::new(builder.references_for_file(file)));
            rebuilt_files += 1;
        }
        for file in &project.files {
            if self.uri_references_by_path.contains_key(&file.path) {
                continue;
            }
            self.uri_references_by_path.insert(
                file.path.clone(),
                Arc::new(builder.references_for_file(file)),
            );
            rebuilt_files += 1;
        }
        self.counters.uri_files_rebuilt += rebuilt_files;
        Arc::new(aggregate_uri_graph(&self.uri_references_by_path))
    }

    fn refresh_part_links(
        &mut self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
        uri_graph: &DartUriGraph,
    ) -> Arc<DartPartLinkAnalysis> {
        if !trigger.plan.part_links {
            return Arc::clone(&trigger.old.part_links);
        }
        self.counters.part_link_rebuilds += 1;
        Arc::new(analyze_part_links_with_graph(project, uri_graph))
    }

    /// Refreshes the library path cache and the dependency fingerprints derived from it.
    fn refresh_library_caches(
        &mut self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
        uri_graph: &DartUriGraph,
        part_links: &DartPartLinkAnalysis,
    ) -> Arc<Vec<DartLibraryDependencyFingerprint>> {
        if trigger.plan.part_links || trigger.file_set_changed {
            self.counters.namespace_libraries_rebuilt +=
                refresh_library_path_cache(project, part_links, &mut self.library_paths_by_owner);
        }
        if !(trigger.plan.uri_graph || trigger.plan.part_links || trigger.file_set_changed) {
            return Arc::clone(&trigger.old.library_dependency_fingerprints);
        }
        self.counters.library_dependency_fingerprints_rebuilt +=
            refresh_library_dependency_fingerprint_cache(
                uri_graph,
                &self.library_paths_by_owner,
                &mut self.library_dependency_fingerprints_by_owner,
            );
        Arc::new(aggregate_library_dependency_fingerprints(
            &self.library_dependency_fingerprints_by_owner,
        ))
    }

    /// The paths an update affects: dependents of the changed files, files of the libraries whose
    /// part links changed, and the sources that reference a declaration name that changed.
    fn collect_affected_paths(
        &self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
        uri_graph: &DartUriGraph,
        part_links: &DartPartLinkAnalysis,
    ) -> Vec<String> {
        let mut paths: BTreeSet<_> = affected_paths(
            &trigger.changed_paths,
            &trigger.old.uri_graph,
            uri_graph,
            project,
            trigger.global_invalidation,
            trigger.plan.propagate_dependents,
        )
        .into_iter()
        .collect();
        if trigger.plan.part_links {
            paths.extend(library_related_paths(
                &trigger.changed_paths,
                trigger.old.part_links.as_ref(),
                part_links,
            ));
        }
        paths.extend(reference_sources_for_declaration_names(
            &self.references_by_path,
            &trigger.changed_declaration_names,
        ));
        paths.into_iter().collect()
    }

    /// Re-analyzes the GraphQL contracts of the libraries the update invalidated or created.
    fn refresh_graphql_contracts(
        &mut self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
        uri_graph: &Arc<DartUriGraph>,
        part_links: &DartPartLinkAnalysis,
        affected_paths: &[String],
    ) -> Arc<DartGraphqlContractAnalysis> {
        if !trigger.plan.graphql_contracts {
            return Arc::clone(&trigger.old.graphql_contracts);
        }
        self.counters.graphql_rebuilds += 1;
        let active_libraries = graphql_library_owners(project, &self.library_paths_by_owner);
        let rebuild_libraries = graphql_rebuild_libraries(
            &trigger.changed_paths,
            affected_paths,
            &trigger.changed_graphql_operation_names,
            ProjectLinks {
                project: trigger.old.project.as_ref(),
                part_links: trigger.old.part_links.as_ref(),
            },
            ProjectLinks {
                project,
                part_links,
            },
            trigger.global_invalidation,
        );
        let mut rebuilt_libraries = 0_u64;
        if trigger.global_invalidation {
            self.graphql_contracts_by_library.clear();
        } else {
            let before = self.graphql_contracts_by_library.len();
            self.graphql_contracts_by_library
                .retain(|owner, _| active_libraries.contains(owner));
            rebuilt_libraries += (before - self.graphql_contracts_by_library.len()) as u64;
        }
        let analyzer = GraphqlContractAnalyzer::from_analyses(
            project,
            &self.options,
            Arc::clone(uri_graph),
            part_links,
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
    }

    /// Re-resolves the identifier references of the files that may have changed.
    fn refresh_reference_resolutions(
        &mut self,
        trigger: &RebuildTrigger<'_>,
        project: &DartProjectAnalysis,
        affected_paths: &[String],
    ) -> Arc<DartIdentifierReferenceResolutionAnalysis> {
        if !trigger.plan.identifier_references {
            return Arc::clone(&trigger.old.identifier_reference_resolutions);
        }
        self.counters.reference_rebuilds += 1;
        let rebuild_paths = reference_rebuild_paths(
            &trigger.changed_paths,
            affected_paths,
            &self.references_by_path,
            trigger.global_invalidation,
            trigger.plan.propagate_dependents,
        );
        if trigger.global_invalidation {
            self.reference_resolutions_by_path.clear();
        }
        let references_by_path = &self.references_by_path;
        self.reference_resolutions_by_path
            .retain(|path, _| references_by_path.contains_key(path));
        let mut rebuilt_files = 0_u64;
        for path in rebuild_paths {
            let Some(references) = self.references_by_path.get(&path) else {
                self.reference_resolutions_by_path.remove(&path);
                continue;
            };
            let analysis =
                resolve_identifier_references_with_options(project, references, &self.options);
            self.reference_resolutions_by_path
                .insert(path, Arc::new(analysis.resolutions));
            rebuilt_files += 1;
        }
        for (path, references) in &self.references_by_path {
            if self.reference_resolutions_by_path.contains_key(path) {
                continue;
            }
            let analysis =
                resolve_identifier_references_with_options(project, references, &self.options);
            self.reference_resolutions_by_path
                .insert(path.clone(), Arc::new(analysis.resolutions));
            rebuilt_files += 1;
        }
        self.counters.reference_files_rebuilt += rebuilt_files;
        Arc::new(aggregate_reference_resolutions(
            &self.reference_resolutions_by_path,
        ))
    }
}
