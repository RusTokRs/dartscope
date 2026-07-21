use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use dartscope_core::{
    DartDiagnostic, DartFileAnalysis, DartFileReferenceAnalysis, DartGraphqlContractAnalysis,
    DartIdentifierReference, DartIdentifierReferenceResolution,
    DartIdentifierReferenceResolutionAnalysis, DartLexicalBinding, DartPartLinkAnalysis,
    DartPartLinkStatus, DartProjectAnalysis, DartProjectReferenceAnalysis, DartProjectSummary,
    DartUriGraph, DartUriReference, DartUriReferenceKind, PackageConfigAnalysis, PubspecAnalysis,
    normalize_path,
};

use crate::graphql::{GraphqlContractAnalyzer, sort_contract_analysis};
use crate::namespace::LibraryMembership;
use crate::parts::analyze_part_links_with_graph;
use crate::references::resolve_identifier_references_with_options;
use crate::uri_graph::{DartIndexOptions, UriGraphBuilder, sort_uri_references};

/// Stable import/export dependency evidence for one normalized Dart library.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartLibraryDependencyFingerprint {
    pub owner_path: String,
    pub member_paths: Vec<String>,
    pub references: Vec<DartUriReference>,
}

/// Immutable, shareable view of one workspace-index generation.
///
/// The mutable [`DartWorkspaceIndex`] owns normalized analysis inputs. Snapshots own `Arc` handles to
/// derived products, so unchanged products are reused between generations and remain valid while the
/// mutable index advances.
#[derive(Debug, Clone)]
pub struct DartWorkspaceSnapshot {
    generation: u64,
    project: Arc<DartProjectAnalysis>,
    uri_graph: Arc<DartUriGraph>,
    part_links: Arc<DartPartLinkAnalysis>,
    library_dependency_fingerprints: Arc<Vec<DartLibraryDependencyFingerprint>>,
    graphql_contracts: Arc<DartGraphqlContractAnalysis>,
    identifier_reference_resolutions: Arc<DartIdentifierReferenceResolutionAnalysis>,
    identifier_references: Arc<Vec<DartIdentifierReference>>,
    lexical_bindings: Arc<Vec<DartLexicalBinding>>,
    options: DartIndexOptions,
}

impl DartWorkspaceSnapshot {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn project(&self) -> &DartProjectAnalysis {
        &self.project
    }

    pub fn uri_graph(&self) -> &DartUriGraph {
        &self.uri_graph
    }

    pub fn part_links(&self) -> &DartPartLinkAnalysis {
        &self.part_links
    }

    pub fn library_dependency_fingerprints(&self) -> &[DartLibraryDependencyFingerprint] {
        self.library_dependency_fingerprints.as_slice()
    }

    pub fn library_dependency_fingerprint(
        &self,
        owner_path: &str,
    ) -> Option<&DartLibraryDependencyFingerprint> {
        let owner_path = normalize_path(owner_path.to_string());
        self.library_dependency_fingerprints
            .binary_search_by(|fingerprint| fingerprint.owner_path.cmp(&owner_path))
            .ok()
            .map(|index| &self.library_dependency_fingerprints[index])
    }

    pub fn graphql_contracts(&self) -> &DartGraphqlContractAnalysis {
        &self.graphql_contracts
    }

    pub fn identifier_reference_resolutions(&self) -> &DartIdentifierReferenceResolutionAnalysis {
        &self.identifier_reference_resolutions
    }

    pub fn identifier_references(&self) -> &[DartIdentifierReference] {
        self.identifier_references.as_slice()
    }

    pub fn lexical_bindings(&self) -> &[DartLexicalBinding] {
        self.lexical_bindings.as_slice()
    }

    pub fn options(&self) -> &DartIndexOptions {
        &self.options
    }

    pub fn project_reference_analysis(&self) -> DartProjectReferenceAnalysis {
        DartProjectReferenceAnalysis {
            project: self.project.as_ref().clone(),
            references: self.identifier_references.as_ref().clone(),
            bindings: self.lexical_bindings.as_ref().clone(),
        }
    }
}

/// Observable operation counts for deterministic incremental baselines.
///
/// These counters describe semantic work, not wall-clock time. They are suitable for tests and
/// reproducible 1k/10k-file baselines without turning host timing variance into a correctness gate.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartWorkspaceIndexCounters {
    pub generations: u64,
    pub no_op_updates: u64,
    pub project_rebuilds: u64,
    pub uri_graph_rebuilds: u64,
    pub uri_files_rebuilt: u64,
    pub part_link_rebuilds: u64,
    pub namespace_libraries_rebuilt: u64,
    pub library_dependency_fingerprints_rebuilt: u64,
    pub graphql_rebuilds: u64,
    pub graphql_libraries_rebuilt: u64,
    pub reference_rebuilds: u64,
    pub reference_files_rebuilt: u64,
}

/// Deterministic retained-cache shape for memory baselines.
///
/// `retained_path_uri_bytes` is the exact UTF-8 payload retained by cache keys and path/URI evidence.
/// It is a stable lower-bound payload metric, not an allocator-specific heap measurement.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartWorkspaceIndexRetainedMetrics {
    pub indexed_files: usize,
    pub uri_source_entries: usize,
    pub uri_references: usize,
    pub library_entries: usize,
    pub library_member_paths: usize,
    pub dependency_fingerprints: usize,
    pub dependency_references: usize,
    pub graphql_library_entries: usize,
    pub graphql_bindings: usize,
    pub graphql_unresolved_uses: usize,
    pub reference_source_entries: usize,
    pub reference_resolutions: usize,
    pub retained_path_uri_bytes: usize,
}

/// Derived products rebuilt by one workspace mutation.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartWorkspaceSubsystems {
    pub project: bool,
    pub uri_graph: bool,
    pub part_links: bool,
    pub graphql_contracts: bool,
    pub identifier_references: bool,
}

impl DartWorkspaceSubsystems {
    pub const fn any(self) -> bool {
        self.project
            || self.uri_graph
            || self.part_links
            || self.graphql_contracts
            || self.identifier_references
    }
}

/// Deterministic invalidation evidence returned by a state mutation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartWorkspaceUpdate {
    pub generation: u64,
    pub changed_paths: Vec<String>,
    pub affected_paths: Vec<String>,
    pub affected_libraries: Vec<String>,
    pub rebuilt: DartWorkspaceSubsystems,
}

impl DartWorkspaceUpdate {
    pub fn is_no_op(&self) -> bool {
        !self.rebuilt.any() && self.changed_paths.is_empty()
    }
}

/// Stateful index over normalized DartScope analysis models.
///
/// This type performs no filesystem access and never stores parser ASTs. Mutation requires `&mut
/// self`; callers that need shared mutation choose their own synchronization policy. Snapshots are
/// immutable and may be shared across threads independently of later updates.
#[derive(Debug)]
pub struct DartWorkspaceIndex {
    root: String,
    files: BTreeMap<String, DartFileAnalysis>,
    pubspecs: BTreeMap<String, PubspecAnalysis>,
    package_configs: BTreeMap<String, PackageConfigAnalysis>,
    project_diagnostics: Vec<DartDiagnostic>,
    references_by_path: BTreeMap<String, Vec<DartIdentifierReference>>,
    bindings_by_path: BTreeMap<String, Vec<DartLexicalBinding>>,
    uri_references_by_path: BTreeMap<String, Arc<Vec<DartUriReference>>>,
    library_paths_by_owner: BTreeMap<String, Arc<Vec<String>>>,
    library_dependency_fingerprints_by_owner:
        BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
    graphql_contracts_by_library: BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
    reference_resolutions_by_path: BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
    options: DartIndexOptions,
    snapshot: Arc<DartWorkspaceSnapshot>,
    counters: DartWorkspaceIndexCounters,
}

impl DartWorkspaceIndex {
    /// Builds a stateful index from an existing normalized project analysis.
    pub fn from_project(project: DartProjectAnalysis) -> Self {
        Self::from_project_with_options(project, DartIndexOptions::default())
    }

    /// Builds a stateful index with an explicit conditional-compilation environment.
    pub fn from_project_with_options(
        project: DartProjectAnalysis,
        options: DartIndexOptions,
    ) -> Self {
        Self::from_inputs(project, BTreeMap::new(), BTreeMap::new(), options)
    }

    /// Builds a stateful index including opt-in parser-produced identifier references.
    pub fn from_reference_project(analysis: DartProjectReferenceAnalysis) -> Self {
        Self::from_reference_project_with_options(analysis, DartIndexOptions::default())
    }

    /// Builds a stateful reference index with an explicit conditional-compilation environment.
    pub fn from_reference_project_with_options(
        analysis: DartProjectReferenceAnalysis,
        options: DartIndexOptions,
    ) -> Self {
        let references_by_path = group_references(analysis.references);
        let bindings_by_path = group_bindings(analysis.bindings);
        Self::from_inputs(
            analysis.project,
            references_by_path,
            bindings_by_path,
            options,
        )
    }

    fn from_inputs(
        project: DartProjectAnalysis,
        references_by_path: BTreeMap<String, Vec<DartIdentifierReference>>,
        bindings_by_path: BTreeMap<String, Vec<DartLexicalBinding>>,
        options: DartIndexOptions,
    ) -> Self {
        let project_diagnostics = additional_project_diagnostics(&project);
        let root = normalize_path(project.root);
        let files = project
            .files
            .into_iter()
            .map(normalize_file)
            .map(|file| (file.path.clone(), file))
            .collect();
        let pubspecs = project
            .pubspecs
            .into_iter()
            .map(normalize_pubspec)
            .map(|pubspec| (pubspec.path.clone(), pubspec))
            .collect();
        let package_configs = project
            .package_configs
            .into_iter()
            .map(normalize_package_config)
            .map(|config| (config.path.clone(), config))
            .collect();
        let project = Arc::new(build_project(
            &root,
            &files,
            &pubspecs,
            &package_configs,
            &project_diagnostics,
        ));
        let (uri_references_by_path, uri_graph) = build_uri_reference_cache(&project, &options);
        let uri_graph = Arc::new(uri_graph);
        let part_links = Arc::new(analyze_part_links_with_graph(&project, &uri_graph));
        let library_paths_by_owner = build_library_path_cache(&project, &part_links);
        let library_dependency_fingerprints_by_owner =
            build_library_dependency_fingerprint_cache(&uri_graph, &library_paths_by_owner);
        let library_dependency_fingerprints = Arc::new(aggregate_library_dependency_fingerprints(
            &library_dependency_fingerprints_by_owner,
        ));
        let (graphql_contracts_by_library, graphql_contracts) = build_graphql_contract_cache(
            &project,
            &options,
            Arc::clone(&uri_graph),
            &part_links,
            &library_paths_by_owner,
        );
        let graphql_contracts = Arc::new(graphql_contracts);
        let (reference_resolutions_by_path, identifier_reference_resolutions) =
            build_reference_resolution_cache(&project, &references_by_path, &options);
        let identifier_reference_resolutions = Arc::new(identifier_reference_resolutions);
        let identifier_references = Arc::new(aggregate_references(&references_by_path));
        let lexical_bindings = Arc::new(aggregate_bindings(&bindings_by_path));
        let initial_uri_files = uri_references_by_path.len() as u64;
        let initial_namespace_libraries = library_paths_by_owner.len() as u64;
        let initial_dependency_fingerprints = library_dependency_fingerprints_by_owner.len() as u64;
        let initial_graphql_libraries = graphql_contracts_by_library.len() as u64;
        let initial_reference_files = reference_resolutions_by_path.len() as u64;
        let snapshot = Arc::new(DartWorkspaceSnapshot {
            generation: 0,
            project,
            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
            identifier_reference_resolutions,
            identifier_references,
            lexical_bindings,
            options: options.clone(),
        });

        Self {
            root,
            files,
            pubspecs,
            package_configs,
            project_diagnostics,
            references_by_path,
            bindings_by_path,
            uri_references_by_path,
            library_paths_by_owner,
            library_dependency_fingerprints_by_owner,
            graphql_contracts_by_library,
            reference_resolutions_by_path,
            options,
            snapshot,
            counters: DartWorkspaceIndexCounters {
                project_rebuilds: 1,
                uri_graph_rebuilds: 1,
                uri_files_rebuilt: initial_uri_files,
                part_link_rebuilds: 1,
                namespace_libraries_rebuilt: initial_namespace_libraries,
                library_dependency_fingerprints_rebuilt: initial_dependency_fingerprints,
                graphql_rebuilds: 1,
                graphql_libraries_rebuilt: initial_graphql_libraries,
                reference_rebuilds: 1,
                reference_files_rebuilt: initial_reference_files,
                ..DartWorkspaceIndexCounters::default()
            },
        }
    }

    /// Returns the current immutable generation. Previously returned snapshots remain valid.
    pub fn snapshot(&self) -> Arc<DartWorkspaceSnapshot> {
        Arc::clone(&self.snapshot)
    }

    pub const fn counters(&self) -> DartWorkspaceIndexCounters {
        self.counters
    }

    pub fn retained_metrics(&self) -> DartWorkspaceIndexRetainedMetrics {
        let uri_references = self
            .uri_references_by_path
            .values()
            .map(|references| references.len())
            .sum();
        let library_member_paths = self
            .library_paths_by_owner
            .values()
            .map(|paths| paths.len())
            .sum();
        let dependency_references = self
            .library_dependency_fingerprints_by_owner
            .values()
            .map(|fingerprint| fingerprint.references.len())
            .sum();
        let graphql_bindings = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.bindings.len())
            .sum();
        let graphql_unresolved_uses = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.unresolved_uses.len())
            .sum();
        let reference_resolutions = self
            .reference_resolutions_by_path
            .values()
            .map(|resolutions| resolutions.len())
            .sum();
        let retained_path_uri_bytes = self.root.len()
            + self
                .uri_references_by_path
                .iter()
                .map(|(path, references)| {
                    path.len()
                        + references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .library_paths_by_owner
                .iter()
                .map(|(owner, paths)| owner.len() + paths.iter().map(String::len).sum::<usize>())
                .sum::<usize>()
            + self
                .library_dependency_fingerprints_by_owner
                .iter()
                .map(|(owner, fingerprint)| {
                    owner.len()
                        + fingerprint.owner_path.len()
                        + fingerprint
                            .member_paths
                            .iter()
                            .map(String::len)
                            .sum::<usize>()
                        + fingerprint
                            .references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .graphql_contracts_by_library
                .iter()
                .map(|(owner, analysis)| owner.len() + graphql_contract_path_bytes(analysis))
                .sum::<usize>()
            + self
                .reference_resolutions_by_path
                .iter()
                .map(|(path, resolutions)| {
                    path.len()
                        + resolutions
                            .iter()
                            .map(reference_resolution_path_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>();

        DartWorkspaceIndexRetainedMetrics {
            indexed_files: self.files.len(),
            uri_source_entries: self.uri_references_by_path.len(),
            uri_references,
            library_entries: self.library_paths_by_owner.len(),
            library_member_paths,
            dependency_fingerprints: self.library_dependency_fingerprints_by_owner.len(),
            dependency_references,
            graphql_library_entries: self.graphql_contracts_by_library.len(),
            graphql_bindings,
            graphql_unresolved_uses,
            reference_source_entries: self.reference_resolutions_by_path.len(),
            reference_resolutions,
            retained_path_uri_bytes,
        }
    }

    pub fn options(&self) -> &DartIndexOptions {
        &self.options
    }

    /// Inserts or replaces a normalized file analysis and clears stale reference facts for that path.
    pub fn upsert_file(&mut self, file: DartFileAnalysis) -> DartWorkspaceUpdate {
        self.upsert_file_internal(file, None, None)
    }

    /// Inserts or replaces a file together with parser-produced identifier-reference facts.
    pub fn upsert_file_with_references(
        &mut self,
        analysis: DartFileReferenceAnalysis,
    ) -> DartWorkspaceUpdate {
        self.upsert_file_internal(
            analysis.file,
            Some(analysis.references),
            Some(analysis.bindings),
        )
    }

    fn upsert_file_internal(
        &mut self,
        file: DartFileAnalysis,
        references: Option<Vec<DartIdentifierReference>>,
        bindings: Option<Vec<DartLexicalBinding>>,
    ) -> DartWorkspaceUpdate {
        let file = normalize_file(file);
        let path = file.path.clone();
        let new_references = references
            .map(|references| normalize_references_for_path(&path, references))
            .unwrap_or_default();
        let new_bindings = bindings
            .map(|bindings| normalize_bindings_for_path(&path, bindings))
            .unwrap_or_default();
        let old_file = self.files.get(&path).cloned();
        let old_references = self
            .references_by_path
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let old_bindings = self
            .bindings_by_path
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let references_changed = old_references != new_references;
        let bindings_changed = old_bindings != new_bindings;

        if old_file.as_ref() == Some(&file) && !references_changed && !bindings_changed {
            return self.no_op_update();
        }

        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed || bindings_changed),
            None => RebuildPlan::all(),
        };
        let changed_declaration_names =
            changed_top_level_declaration_names(old_file.as_ref(), Some(&file));
        let changed_graphql_operation_names =
            changed_graphql_operation_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
        if new_references.is_empty() {
            self.references_by_path.remove(&path);
        } else {
            self.references_by_path.insert(path.clone(), new_references);
        }
        if new_bindings.is_empty() {
            self.bindings_by_path.remove(&path);
        } else {
            self.bindings_by_path.insert(path.clone(), new_bindings);
        }
        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
            changed_graphql_operation_names,
        )
    }

    /// Removes a file and its opt-in reference facts.
    pub fn remove_file(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        let Some(removed) = self.files.remove(&path) else {
            return self.no_op_update();
        };
        let changed_declaration_names = changed_top_level_declaration_names(Some(&removed), None);
        let changed_graphql_operation_names = changed_graphql_operation_names(Some(&removed), None);
        self.references_by_path.remove(&path);
        self.bindings_by_path.remove(&path);
        self.rebuild(
            RebuildPlan::all(),
            BTreeSet::from([path]),
            false,
            true,
            changed_declaration_names,
            changed_graphql_operation_names,
        )
    }

    /// Inserts or replaces one pubspec analysis.
    pub fn upsert_pubspec(&mut self, pubspec: PubspecAnalysis) -> DartWorkspaceUpdate {
        let pubspec = normalize_pubspec(pubspec);
        let path = pubspec.path.clone();
        let old = self.pubspecs.get(&path).cloned();
        if old.as_ref() == Some(&pubspec) {
            return self.no_op_update();
        }
        let resolution_changed = old
            .as_ref()
            .map(|old| old.package_name != pubspec.package_name)
            .unwrap_or(true);
        self.pubspecs.insert(path.clone(), pubspec);
        self.rebuild(
            RebuildPlan::metadata(resolution_changed),
            BTreeSet::from([path]),
            resolution_changed,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Removes one pubspec analysis by normalized path.
    pub fn remove_pubspec(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.pubspecs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Inserts or replaces one parsed `.dart_tool/package_config.json` analysis.
    pub fn upsert_package_config(&mut self, config: PackageConfigAnalysis) -> DartWorkspaceUpdate {
        let config = normalize_package_config(config);
        let path = config.path.clone();
        if self.package_configs.get(&path) == Some(&config) {
            return self.no_op_update();
        }
        self.package_configs.insert(path.clone(), config);
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Removes one package configuration by normalized path.
    pub fn remove_package_config(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.package_configs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Replaces conditional-compilation options without changing normalized project inputs.
    pub fn update_options(&mut self, options: DartIndexOptions) -> DartWorkspaceUpdate {
        if self.options == options {
            return self.no_op_update();
        }
        self.options = options;
        self.rebuild(
            RebuildPlan::options(),
            BTreeSet::new(),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Changes only the informational project root retained in snapshots.
    pub fn update_root(&mut self, root: impl Into<String>) -> DartWorkspaceUpdate {
        let root = normalize_path(root.into());
        if self.root == root {
            return self.no_op_update();
        }
        self.root = root;
        self.rebuild(
            RebuildPlan::project_only(),
            BTreeSet::new(),
            false,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    fn no_op_update(&mut self) -> DartWorkspaceUpdate {
        self.counters.no_op_updates += 1;
        DartWorkspaceUpdate {
            generation: self.snapshot.generation,
            changed_paths: Vec::new(),
            affected_paths: Vec::new(),
            affected_libraries: Vec::new(),
            rebuilt: DartWorkspaceSubsystems::default(),
        }
    }

    fn rebuild(
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
        let project = if plan.project {
            self.counters.project_rebuilds += 1;
            Arc::new(build_project(
                &self.root,
                &self.files,
                &self.pubspecs,
                &self.package_configs,
                &self.project_diagnostics,
            ))
        } else {
            Arc::clone(&old.project)
        };

        let uri_graph = if plan.uri_graph {
            self.counters.uri_graph_rebuilds += 1;
            let rebuild_paths = uri_rebuild_paths(
                &changed_paths,
                &old.uri_graph,
                &project,
                global_invalidation,
                file_set_changed,
            );
            let options = self.options.clone();
            let builder = UriGraphBuilder::new(&project, &options);
            if global_invalidation {
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
        } else {
            Arc::clone(&old.uri_graph)
        };

        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links_with_graph(&project, &uri_graph))
        } else {
            Arc::clone(&old.part_links)
        };
        if plan.part_links || file_set_changed {
            self.counters.namespace_libraries_rebuilt +=
                refresh_library_path_cache(&project, &part_links, &mut self.library_paths_by_owner);
        }
        let library_dependency_fingerprints =
            if plan.uri_graph || plan.part_links || file_set_changed {
                self.counters.library_dependency_fingerprints_rebuilt +=
                    refresh_library_dependency_fingerprint_cache(
                        &uri_graph,
                        &self.library_paths_by_owner,
                        &mut self.library_dependency_fingerprints_by_owner,
                    );
                Arc::new(aggregate_library_dependency_fingerprints(
                    &self.library_dependency_fingerprints_by_owner,
                ))
            } else {
                Arc::clone(&old.library_dependency_fingerprints)
            };
        let mut affected_paths: BTreeSet<_> = affected_paths(
            &changed_paths,
            &old.uri_graph,
            &uri_graph,
            &project,
            global_invalidation,
            plan.propagate_dependents,
        )
        .into_iter()
        .collect();
        if plan.part_links {
            affected_paths.extend(library_related_paths(
                &changed_paths,
                old.part_links.as_ref(),
                part_links.as_ref(),
            ));
        }
        affected_paths.extend(reference_sources_for_declaration_names(
            &self.references_by_path,
            &changed_declaration_names,
        ));
        let affected_paths: Vec<_> = affected_paths.into_iter().collect();
        let affected_libraries = affected_library_owners(
            &changed_paths,
            &affected_paths,
            &old.project,
            &old.part_links,
            &project,
            &part_links,
        );
        let graphql_contracts = if plan.graphql_contracts {
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
                rebuilt_libraries += (before - self.graphql_contracts_by_library.len()) as u64;
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
        let identifier_reference_resolutions = if plan.identifier_references {
            self.counters.reference_rebuilds += 1;
            let rebuild_paths = reference_rebuild_paths(
                &changed_paths,
                &affected_paths,
                &self.references_by_path,
                global_invalidation,
                plan.propagate_dependents,
            );
            if global_invalidation {
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
                    resolve_identifier_references_with_options(&project, references, &self.options);
                self.reference_resolutions_by_path
                    .insert(path, Arc::new(analysis.resolutions));
                rebuilt_files += 1;
            }
            for (path, references) in &self.references_by_path {
                if self.reference_resolutions_by_path.contains_key(path) {
                    continue;
                }
                let analysis =
                    resolve_identifier_references_with_options(&project, references, &self.options);
                self.reference_resolutions_by_path
                    .insert(path.clone(), Arc::new(analysis.resolutions));
                rebuilt_files += 1;
            }
            self.counters.reference_files_rebuilt += rebuilt_files;
            Arc::new(aggregate_reference_resolutions(
                &self.reference_resolutions_by_path,
            ))
        } else {
            Arc::clone(&old.identifier_reference_resolutions)
        };

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
            changed_paths: changed_paths.into_iter().collect(),
            affected_paths,
            affected_libraries,
            rebuilt: plan.public(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RebuildPlan {
    project: bool,
    uri_graph: bool,
    part_links: bool,
    graphql_contracts: bool,
    identifier_references: bool,
    propagate_dependents: bool,
}

impl RebuildPlan {
    const fn all() -> Self {
        Self {
            project: true,
            uri_graph: true,
            part_links: true,
            graphql_contracts: true,
            identifier_references: true,
            propagate_dependents: true,
        }
    }

    const fn project_only() -> Self {
        Self {
            project: true,
            uri_graph: false,
            part_links: false,
            graphql_contracts: false,
            identifier_references: false,
            propagate_dependents: false,
        }
    }

    const fn metadata(resolution_changed: bool) -> Self {
        Self {
            project: true,
            uri_graph: resolution_changed,
            part_links: resolution_changed,
            graphql_contracts: resolution_changed,
            identifier_references: resolution_changed,
            propagate_dependents: resolution_changed,
        }
    }

    const fn options() -> Self {
        Self {
            project: false,
            uri_graph: true,
            part_links: false,
            graphql_contracts: true,
            identifier_references: true,
            propagate_dependents: true,
        }
    }

    const fn public(self) -> DartWorkspaceSubsystems {
        DartWorkspaceSubsystems {
            project: self.project,
            uri_graph: self.uri_graph,
            part_links: self.part_links,
            graphql_contracts: self.graphql_contracts,
            identifier_references: self.identifier_references,
        }
    }
}

fn file_rebuild_plan(
    old: &DartFileAnalysis,
    new: &DartFileAnalysis,
    references_changed: bool,
) -> RebuildPlan {
    let file_changed = old != new;
    let import_export_changed = old.imports != new.imports || old.exports != new.exports;
    let part_directives_changed = old.parts != new.parts;
    let library_membership_changed = old.library != new.library || old.part_of != new.part_of;
    let namespace_changed =
        import_export_changed || part_directives_changed || library_membership_changed;
    let top_level_declarations_changed =
        top_level_declaration_facts(old) != top_level_declaration_facts(new);
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
        project: file_changed,
        uri_graph: import_export_changed || part_directives_changed,
        part_links: part_directives_changed || library_membership_changed,
        graphql_contracts: namespace_changed
            || graphql_operations_changed
            || old.graphql_operation_uses != new.graphql_operation_uses,
        identifier_references: namespace_changed
            || top_level_declarations_changed
            || references_changed,
        propagate_dependents: namespace_changed
            || top_level_declarations_changed
            || graphql_operations_changed,
    }
}

fn changed_graphql_operation_names(
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
    file: &DartFileAnalysis,
) -> Vec<(
    &str,
    dartscope_core::DartDeclarationKind,
    Option<&str>,
    &dartscope_core::SourceSpan,
)> {
    file.declarations
        .iter()
        .filter(|declaration| declaration.parent_symbol_id.is_none())
        .map(|declaration| {
            (
                declaration.name.as_str(),
                declaration.kind,
                declaration.symbol_id.as_deref(),
                &declaration.span,
            )
        })
        .collect()
}

fn changed_top_level_declaration_names(
    old: Option<&DartFileAnalysis>,
    new: Option<&DartFileAnalysis>,
) -> BTreeSet<String> {
    let old_facts = old.map(top_level_declaration_facts).unwrap_or_default();
    let new_facts = new.map(top_level_declaration_facts).unwrap_or_default();
    if old_facts == new_facts {
        return BTreeSet::new();
    }

    let mut names = BTreeSet::new();
    if let Some(file) = old {
        names.extend(
            file.declarations
                .iter()
                .filter(|declaration| declaration.parent_symbol_id.is_none())
                .map(|declaration| declaration.name.clone()),
        );
    }
    if let Some(file) = new {
        names.extend(
            file.declarations
                .iter()
                .filter(|declaration| declaration.parent_symbol_id.is_none())
                .map(|declaration| declaration.name.clone()),
        );
    }
    names
}

fn reference_sources_for_declaration_names(
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    names: &BTreeSet<String>,
) -> BTreeSet<String> {
    if names.is_empty() {
        return BTreeSet::new();
    }
    references_by_path
        .iter()
        .filter(|(_, references)| {
            references
                .iter()
                .any(|reference| names.contains(&reference.name))
        })
        .map(|(path, _)| path.clone())
        .collect()
}

fn library_related_paths(
    changed_paths: &BTreeSet<String>,
    old_links: &DartPartLinkAnalysis,
    new_links: &DartPartLinkAnalysis,
) -> BTreeSet<String> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for link in old_links.links.iter().chain(&new_links.links) {
        if link.status != DartPartLinkStatus::Matched {
            continue;
        }
        let Some(part_path) = link.part_path.as_ref() else {
            continue;
        };
        adjacency
            .entry(link.owner_path.clone())
            .or_default()
            .insert(part_path.clone());
        adjacency
            .entry(part_path.clone())
            .or_default()
            .insert(link.owner_path.clone());
    }

    let mut visited = changed_paths.clone();
    let mut related = BTreeSet::new();
    let mut queue: VecDeque<_> = changed_paths.iter().cloned().collect();
    while let Some(path) = queue.pop_front() {
        let Some(neighbors) = adjacency.get(&path) else {
            continue;
        };
        for neighbor in neighbors {
            if visited.insert(neighbor.clone()) {
                related.insert(neighbor.clone());
                queue.push_back(neighbor.clone());
            }
        }
    }
    related
}

fn grouped_library_paths(
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

fn build_library_dependency_fingerprint_cache(
    uri_graph: &DartUriGraph,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> BTreeMap<String, Arc<DartLibraryDependencyFingerprint>> {
    library_paths
        .iter()
        .map(|(owner, paths)| {
            (
                owner.clone(),
                Arc::new(library_dependency_fingerprint(owner, paths, uri_graph)),
            )
        })
        .collect()
}

fn library_dependency_fingerprint(
    owner: &str,
    member_paths: &[String],
    uri_graph: &DartUriGraph,
) -> DartLibraryDependencyFingerprint {
    let members: BTreeSet<_> = member_paths.iter().map(String::as_str).collect();
    let mut references: Vec<_> = uri_graph
        .references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartUriReferenceKind::Import | DartUriReferenceKind::Export
            ) && members.contains(reference.source_path.as_str())
        })
        .cloned()
        .collect();
    sort_uri_references(&mut references);
    DartLibraryDependencyFingerprint {
        owner_path: owner.to_string(),
        member_paths: member_paths.to_vec(),
        references,
    }
}

fn refresh_library_dependency_fingerprint_cache(
    uri_graph: &DartUriGraph,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
    cache: &mut BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
) -> u64 {
    let desired = build_library_dependency_fingerprint_cache(uri_graph, library_paths);
    let before = cache.len();
    cache.retain(|owner, _| desired.contains_key(owner));
    let mut rebuilt = (before - cache.len()) as u64;
    for (owner, fingerprint) in desired {
        if cache
            .get(&owner)
            .is_some_and(|existing| existing.as_ref() == fingerprint.as_ref())
        {
            continue;
        }
        cache.insert(owner, fingerprint);
        rebuilt += 1;
    }
    rebuilt
}

fn aggregate_library_dependency_fingerprints(
    cache: &BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
) -> Vec<DartLibraryDependencyFingerprint> {
    cache
        .values()
        .map(|fingerprint| fingerprint.as_ref().clone())
        .collect()
}

fn affected_library_owners(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    old_project: &DartProjectAnalysis,
    old_part_links: &DartPartLinkAnalysis,
    new_project: &DartProjectAnalysis,
    new_part_links: &DartPartLinkAnalysis,
) -> Vec<String> {
    let old_files: BTreeSet<_> = old_project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let new_files: BTreeSet<_> = new_project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let old_membership = LibraryMembership::from_part_links(old_part_links);
    let new_membership = LibraryMembership::from_part_links(new_part_links);
    let mut owners = BTreeSet::new();
    for path in changed_paths.iter().chain(affected_paths) {
        if old_files.contains(path.as_str()) {
            owners.insert(old_membership.owner_of(path).to_string());
        }
        if new_files.contains(path.as_str()) {
            owners.insert(new_membership.owner_of(path).to_string());
        }
    }
    owners.into_iter().collect()
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

fn uri_reference_path_uri_bytes(reference: &DartUriReference) -> usize {
    reference.source_path.len()
        + reference.uri.len()
        + reference.condition.as_ref().map_or(0, String::len)
        + reference.target_path.as_ref().map_or(0, String::len)
        + reference.target_uri.as_ref().map_or(0, String::len)
        + reference
            .candidate_paths
            .iter()
            .map(String::len)
            .sum::<usize>()
}

fn graphql_contract_path_bytes(analysis: &DartGraphqlContractAnalysis) -> usize {
    analysis
        .bindings
        .iter()
        .map(|binding| binding.operation_path.len() + binding.use_path.len())
        .sum::<usize>()
        + analysis
            .unresolved_uses
            .iter()
            .map(|operation_use| {
                operation_use.use_path.len()
                    + operation_use
                        .candidate_paths
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
            })
            .sum::<usize>()
}

fn reference_resolution_path_bytes(resolution: &DartIdentifierReferenceResolution) -> usize {
    resolution.reference.source_path.len()
        + resolution
            .candidates
            .iter()
            .map(|candidate| candidate.declaration_path.len())
            .sum::<usize>()
}

fn build_uri_reference_cache(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> (BTreeMap<String, Arc<Vec<DartUriReference>>>, DartUriGraph) {
    let builder = UriGraphBuilder::new(project, options);
    let cache = project
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                Arc::new(builder.references_for_file(file)),
            )
        })
        .collect();
    let graph = aggregate_uri_graph(&cache);
    (cache, graph)
}

fn aggregate_uri_graph(cache: &BTreeMap<String, Arc<Vec<DartUriReference>>>) -> DartUriGraph {
    let mut references: Vec<_> = cache
        .values()
        .flat_map(|references| references.iter().cloned())
        .collect();
    sort_uri_references(&mut references);
    DartUriGraph { references }
}

fn build_reference_resolution_cache(
    project: &DartProjectAnalysis,
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    options: &DartIndexOptions,
) -> (
    BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
    DartIdentifierReferenceResolutionAnalysis,
) {
    let cache = references_by_path
        .iter()
        .map(|(path, references)| {
            let analysis = resolve_identifier_references_with_options(project, references, options);
            (path.clone(), Arc::new(analysis.resolutions))
        })
        .collect();
    let analysis = aggregate_reference_resolutions(&cache);
    (cache, analysis)
}

fn aggregate_reference_resolutions(
    cache: &BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
) -> DartIdentifierReferenceResolutionAnalysis {
    DartIdentifierReferenceResolutionAnalysis {
        resolutions: cache
            .values()
            .flat_map(|resolutions| resolutions.iter().cloned())
            .collect(),
    }
}

fn uri_rebuild_paths(
    changed_paths: &BTreeSet<String>,
    old_graph: &DartUriGraph,
    project: &DartProjectAnalysis,
    global_invalidation: bool,
    file_set_changed: bool,
) -> BTreeSet<String> {
    if global_invalidation {
        return project.files.iter().map(|file| file.path.clone()).collect();
    }

    let known_files: BTreeSet<_> = project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut paths: BTreeSet<_> = changed_paths
        .iter()
        .filter(|path| known_files.contains(path.as_str()))
        .cloned()
        .collect();
    if file_set_changed {
        let reverse = reverse_dependencies(old_graph);
        for changed in changed_paths {
            if let Some(sources) = reverse.get(changed) {
                paths.extend(sources.iter().cloned());
            }
        }
    }
    paths
}

fn reference_rebuild_paths(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    global_invalidation: bool,
    propagate_dependents: bool,
) -> BTreeSet<String> {
    if global_invalidation {
        return references_by_path.keys().cloned().collect();
    }
    let candidates: Box<dyn Iterator<Item = &String> + '_> = if propagate_dependents {
        Box::new(affected_paths.iter())
    } else {
        Box::new(changed_paths.iter())
    };
    candidates
        .filter(|path| references_by_path.contains_key(*path))
        .cloned()
        .collect()
}

fn build_project(
    root: &str,
    files: &BTreeMap<String, DartFileAnalysis>,
    pubspecs: &BTreeMap<String, PubspecAnalysis>,
    package_configs: &BTreeMap<String, PackageConfigAnalysis>,
    project_diagnostics: &[DartDiagnostic],
) -> DartProjectAnalysis {
    let files: Vec<_> = files.values().cloned().collect();
    let pubspecs: Vec<_> = pubspecs.values().cloned().collect();
    let package_configs: Vec<_> = package_configs.values().cloned().collect();
    let diagnostics: Vec<_> = files
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter().cloned())
        .chain(
            pubspecs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter().cloned()),
        )
        .chain(
            package_configs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter().cloned()),
        )
        .chain(project_diagnostics.iter().cloned())
        .collect();
    let summary = DartProjectSummary {
        dart_files: files.len(),
        pubspecs: pubspecs.len(),
        package_configs: package_configs.len(),
        imports: files.iter().map(|analysis| analysis.imports.len()).sum(),
        exports: files.iter().map(|analysis| analysis.exports.len()).sum(),
        parts: files.iter().map(|analysis| analysis.parts.len()).sum(),
        declarations: files
            .iter()
            .map(|analysis| analysis.declarations.len())
            .sum(),
        string_constants: files
            .iter()
            .map(|analysis| analysis.string_constants.len())
            .sum(),
        graphql_operations: files
            .iter()
            .map(|analysis| analysis.graphql_operations.len())
            .sum(),
        graphql_operation_uses: files
            .iter()
            .map(|analysis| analysis.graphql_operation_uses.len())
            .sum(),
        flutter_widgets: files
            .iter()
            .map(|analysis| analysis.flutter.widgets.len())
            .sum(),
        flutter_routes: files
            .iter()
            .map(|analysis| analysis.flutter.routes.len())
            .sum(),
        flutter_assets: files
            .iter()
            .map(|analysis| analysis.flutter.assets.len())
            .sum(),
        flutter_localizations: files
            .iter()
            .map(|analysis| analysis.flutter.localizations.len())
            .sum(),
        package_dependencies: pubspecs
            .iter()
            .map(|analysis| analysis.dependencies.len())
            .sum(),
        diagnostics: diagnostics.len(),
    };

    DartProjectAnalysis {
        root: root.to_string(),
        files,
        pubspecs,
        package_configs,
        summary,
        diagnostics,
    }
}

fn additional_project_diagnostics(project: &DartProjectAnalysis) -> Vec<DartDiagnostic> {
    let child_diagnostics: Vec<_> = project
        .files
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter())
        .chain(
            project
                .pubspecs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter()),
        )
        .chain(
            project
                .package_configs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter()),
        )
        .collect();
    let mut consumed = vec![false; child_diagnostics.len()];
    let mut additional = Vec::new();
    for diagnostic in &project.diagnostics {
        let matched = child_diagnostics
            .iter()
            .enumerate()
            .find(|(index, candidate)| !consumed[*index] && **candidate == diagnostic)
            .map(|(index, _)| index);
        if let Some(index) = matched {
            consumed[index] = true;
        } else {
            additional.push(diagnostic.clone());
        }
    }
    additional
}

fn normalize_file(mut file: DartFileAnalysis) -> DartFileAnalysis {
    file.path = normalize_path(file.path);
    file
}

fn normalize_pubspec(mut pubspec: PubspecAnalysis) -> PubspecAnalysis {
    pubspec.path = normalize_path(pubspec.path);
    pubspec
}

fn normalize_package_config(mut config: PackageConfigAnalysis) -> PackageConfigAnalysis {
    config.path = normalize_path(config.path);
    config
}

fn aggregate_references(
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
) -> Vec<DartIdentifierReference> {
    references_by_path
        .values()
        .flat_map(|references| references.iter().cloned())
        .collect()
}

fn aggregate_bindings(
    bindings_by_path: &BTreeMap<String, Vec<DartLexicalBinding>>,
) -> Vec<DartLexicalBinding> {
    bindings_by_path
        .values()
        .flat_map(|bindings| bindings.iter().cloned())
        .collect()
}

fn group_bindings(bindings: Vec<DartLexicalBinding>) -> BTreeMap<String, Vec<DartLexicalBinding>> {
    let mut grouped: BTreeMap<String, Vec<DartLexicalBinding>> = BTreeMap::new();
    for mut binding in bindings {
        binding.source_path = normalize_path(binding.source_path);
        grouped
            .entry(binding.source_path.clone())
            .or_default()
            .push(binding);
    }
    for bindings in grouped.values_mut() {
        sort_and_deduplicate_bindings(bindings);
    }
    grouped
}

fn group_references(
    references: Vec<DartIdentifierReference>,
) -> BTreeMap<String, Vec<DartIdentifierReference>> {
    let mut grouped: BTreeMap<String, Vec<DartIdentifierReference>> = BTreeMap::new();
    for mut reference in references {
        reference.source_path = normalize_path(reference.source_path);
        grouped
            .entry(reference.source_path.clone())
            .or_default()
            .push(reference);
    }
    for references in grouped.values_mut() {
        sort_and_deduplicate_references(references);
    }
    grouped
}

fn normalize_references_for_path(
    path: &str,
    references: Vec<DartIdentifierReference>,
) -> Vec<DartIdentifierReference> {
    let mut references: Vec<_> = references
        .into_iter()
        .map(|mut reference| {
            reference.source_path = path.to_string();
            reference
        })
        .collect();
    sort_and_deduplicate_references(&mut references);
    references
}

fn normalize_bindings_for_path(
    path: &str,
    bindings: Vec<DartLexicalBinding>,
) -> Vec<DartLexicalBinding> {
    let mut bindings: Vec<_> = bindings
        .into_iter()
        .map(|mut binding| {
            binding.source_path = path.to_string();
            binding
        })
        .collect();
    sort_and_deduplicate_bindings(&mut bindings);
    bindings
}

fn sort_and_deduplicate_bindings(bindings: &mut Vec<DartLexicalBinding>) {
    bindings.sort_by(|left, right| {
        (
            &left.source_path,
            left.declaration_span.byte_start,
            left.declaration_span.byte_end,
            left.kind,
            &left.name,
            &left.symbol_id,
        )
            .cmp(&(
                &right.source_path,
                right.declaration_span.byte_start,
                right.declaration_span.byte_end,
                right.kind,
                &right.name,
                &right.symbol_id,
            ))
    });
    bindings.dedup_by(|left, right| {
        left.source_path == right.source_path
            && left.symbol_id == right.symbol_id
            && left.declaration_span.byte_start == right.declaration_span.byte_start
            && left.declaration_span.byte_end == right.declaration_span.byte_end
    });
}

fn sort_and_deduplicate_references(references: &mut Vec<DartIdentifierReference>) {
    references.sort_by(|left, right| {
        (
            &left.source_path,
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                &right.source_path,
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
    references.dedup();
}

fn affected_paths(
    changed_paths: &BTreeSet<String>,
    old_graph: &DartUriGraph,
    new_graph: &DartUriGraph,
    project: &DartProjectAnalysis,
    global_invalidation: bool,
    dependency_impact: bool,
) -> Vec<String> {
    if global_invalidation {
        return project.files.iter().map(|file| file.path.clone()).collect();
    }
    if !dependency_impact {
        return changed_paths.iter().cloned().collect();
    }

    let mut reverse = reverse_dependencies(old_graph);
    for (target, sources) in reverse_dependencies(new_graph) {
        reverse.entry(target).or_default().extend(sources);
    }
    let mut affected = changed_paths.clone();
    let mut queue: VecDeque<_> = changed_paths.iter().cloned().collect();
    while let Some(target) = queue.pop_front() {
        let Some(sources) = reverse.get(&target) else {
            continue;
        };
        for source in sources {
            if affected.insert(source.clone()) {
                queue.push_back(source.clone());
            }
        }
    }
    affected.into_iter().collect()
}

fn reverse_dependencies(graph: &DartUriGraph) -> BTreeMap<String, BTreeSet<String>> {
    let mut reverse = BTreeMap::new();
    for reference in &graph.references {
        if let Some(target) = &reference.target_path {
            reverse
                .entry(target.clone())
                .or_insert_with(BTreeSet::new)
                .insert(reference.source_path.clone());
        }
        for candidate in &reference.candidate_paths {
            reverse
                .entry(candidate.clone())
                .or_insert_with(BTreeSet::new)
                .insert(reference.source_path.clone());
        }
    }
    reverse
}
