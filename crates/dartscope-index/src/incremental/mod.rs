//! The stateful workspace index and its immutable snapshots.
//!
//! This module holds the public types; the work is split by concern:
//!
//! - `mutations`: the `upsert_*`, `remove_*` and `update_*` API and the planning it triggers;
//! - `plan`: what an update has to rebuild and which paths and libraries it affects;
//! - `rebuild`: one incremental rebuild, phase by phase;
//! - `project`, `caches`, `libraries`, `graphql_cache`: the products and caches the phases maintain;
//! - `metrics`: retained-size metrics.

mod caches;
mod graphql_cache;
mod libraries;
mod metrics;
mod mutations;
mod plan;
mod project;
mod rebuild;

use std::collections::BTreeMap;
use std::sync::Arc;

use dartscope_core::{
    DartDiagnostic, DartFileAnalysis, DartGraphqlContractAnalysis, DartIdentifierReference,
    DartIdentifierReferenceResolution, DartIdentifierReferenceResolutionAnalysis,
    DartLexicalBinding, DartPartLinkAnalysis, DartProjectAnalysis, DartProjectReferenceAnalysis,
    DartUriGraph, DartUriReference, PackageConfigAnalysis, PubspecAnalysis, normalize_path,
};

use crate::parts::analyze_part_links_with_graph;
use crate::uri_graph::DartIndexOptions;

use self::caches::{build_reference_resolution_cache, build_uri_reference_cache};
use self::graphql_cache::build_graphql_contract_cache;
use self::libraries::{
    aggregate_library_dependency_fingerprints, build_library_dependency_fingerprint_cache,
    build_library_path_cache,
};
use self::project::{
    additional_project_diagnostics, aggregate_bindings, aggregate_references, build_project,
    group_bindings, group_references, normalize_file, normalize_package_config, normalize_pubspec,
};

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

    pub fn options(&self) -> &DartIndexOptions {
        &self.options
    }
}
