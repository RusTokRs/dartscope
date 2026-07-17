use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use dartscope_core::{
    DartDiagnostic, DartFileAnalysis, DartFileReferenceAnalysis, DartGraphqlContractAnalysis,
    DartIdentifierReference, DartIdentifierReferenceResolutionAnalysis, DartPartLinkAnalysis,
    DartProjectAnalysis, DartProjectReferenceAnalysis, DartProjectSummary, DartUriGraph,
    PackageConfigAnalysis, PubspecAnalysis, normalize_path,
};

use crate::graphql::analyze_graphql_contracts_with_options;
use crate::parts::analyze_part_links;
use crate::references::resolve_identifier_references_with_options;
use crate::uri_graph::{DartIndexOptions, build_uri_graph_with_options};

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
    graphql_contracts: Arc<DartGraphqlContractAnalysis>,
    identifier_reference_resolutions: Arc<DartIdentifierReferenceResolutionAnalysis>,
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

    pub fn graphql_contracts(&self) -> &DartGraphqlContractAnalysis {
        &self.graphql_contracts
    }

    pub fn identifier_reference_resolutions(&self) -> &DartIdentifierReferenceResolutionAnalysis {
        &self.identifier_reference_resolutions
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
    pub part_link_rebuilds: u64,
    pub graphql_rebuilds: u64,
    pub reference_rebuilds: u64,
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
        Self::from_inputs(project, BTreeMap::new(), options)
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
        Self::from_inputs(analysis.project, references_by_path, options)
    }

    fn from_inputs(
        project: DartProjectAnalysis,
        references_by_path: BTreeMap<String, Vec<DartIdentifierReference>>,
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
        let uri_graph = Arc::new(build_uri_graph_with_options(&project, &options));
        let part_links = Arc::new(analyze_part_links(&project));
        let graphql_contracts =
            Arc::new(analyze_graphql_contracts_with_options(&project, &options));
        let references = flatten_references(&references_by_path);
        let identifier_reference_resolutions = Arc::new(
            resolve_identifier_references_with_options(&project, &references, &options),
        );
        let snapshot = Arc::new(DartWorkspaceSnapshot {
            generation: 0,
            project,
            uri_graph,
            part_links,
            graphql_contracts,
            identifier_reference_resolutions,
        });

        Self {
            root,
            files,
            pubspecs,
            package_configs,
            project_diagnostics,
            references_by_path,
            options,
            snapshot,
            counters: DartWorkspaceIndexCounters {
                project_rebuilds: 1,
                uri_graph_rebuilds: 1,
                part_link_rebuilds: 1,
                graphql_rebuilds: 1,
                reference_rebuilds: 1,
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

    /// Inserts or replaces a normalized file analysis and clears stale reference facts for that path.
    pub fn upsert_file(&mut self, file: DartFileAnalysis) -> DartWorkspaceUpdate {
        self.upsert_file_internal(file, None)
    }

    /// Inserts or replaces a file together with parser-produced identifier-reference facts.
    pub fn upsert_file_with_references(
        &mut self,
        analysis: DartFileReferenceAnalysis,
    ) -> DartWorkspaceUpdate {
        self.upsert_file_internal(analysis.file, Some(analysis.references))
    }

    fn upsert_file_internal(
        &mut self,
        file: DartFileAnalysis,
        references: Option<Vec<DartIdentifierReference>>,
    ) -> DartWorkspaceUpdate {
        let file = normalize_file(file);
        let path = file.path.clone();
        let new_references = references
            .map(|references| normalize_references_for_path(&path, references))
            .unwrap_or_default();
        let old_file = self.files.get(&path).cloned();
        let old_references = self
            .references_by_path
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let references_changed = old_references != new_references;

        if old_file.as_ref() == Some(&file) && !references_changed {
            return self.no_op_update();
        }

        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed),
            None => RebuildPlan::all(),
        };
        self.files.insert(path.clone(), file);
        if new_references.is_empty() {
            self.references_by_path.remove(&path);
        } else {
            self.references_by_path.insert(path.clone(), new_references);
        }
        self.rebuild(plan, BTreeSet::from([path]), false)
    }

    /// Removes a file and its opt-in reference facts.
    pub fn remove_file(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.files.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.references_by_path.remove(&path);
        self.rebuild(RebuildPlan::all(), BTreeSet::from([path]), false)
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
        )
    }

    /// Removes one pubspec analysis by normalized path.
    pub fn remove_pubspec(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.pubspecs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(RebuildPlan::metadata(true), BTreeSet::from([path]), true)
    }

    /// Inserts or replaces one parsed `.dart_tool/package_config.json` analysis.
    pub fn upsert_package_config(&mut self, config: PackageConfigAnalysis) -> DartWorkspaceUpdate {
        let config = normalize_package_config(config);
        let path = config.path.clone();
        if self.package_configs.get(&path) == Some(&config) {
            return self.no_op_update();
        }
        self.package_configs.insert(path.clone(), config);
        self.rebuild(RebuildPlan::metadata(true), BTreeSet::from([path]), true)
    }

    /// Removes one package configuration by normalized path.
    pub fn remove_package_config(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.package_configs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(RebuildPlan::metadata(true), BTreeSet::from([path]), true)
    }

    /// Replaces conditional-compilation options without changing normalized project inputs.
    pub fn update_options(&mut self, options: DartIndexOptions) -> DartWorkspaceUpdate {
        if self.options == options {
            return self.no_op_update();
        }
        self.options = options;
        self.rebuild(RebuildPlan::options(), BTreeSet::new(), true)
    }

    /// Changes only the informational project root retained in snapshots.
    pub fn update_root(&mut self, root: impl Into<String>) -> DartWorkspaceUpdate {
        let root = normalize_path(root.into());
        if self.root == root {
            return self.no_op_update();
        }
        self.root = root;
        self.rebuild(RebuildPlan::project_only(), BTreeSet::new(), false)
    }

    fn no_op_update(&mut self) -> DartWorkspaceUpdate {
        self.counters.no_op_updates += 1;
        DartWorkspaceUpdate {
            generation: self.snapshot.generation,
            changed_paths: Vec::new(),
            affected_paths: Vec::new(),
            rebuilt: DartWorkspaceSubsystems::default(),
        }
    }

    fn rebuild(
        &mut self,
        plan: RebuildPlan,
        changed_paths: BTreeSet<String>,
        global_invalidation: bool,
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
            Arc::new(build_uri_graph_with_options(&project, &self.options))
        } else {
            Arc::clone(&old.uri_graph)
        };
        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links(&project))
        } else {
            Arc::clone(&old.part_links)
        };
        let graphql_contracts = if plan.graphql_contracts {
            self.counters.graphql_rebuilds += 1;
            Arc::new(analyze_graphql_contracts_with_options(
                &project,
                &self.options,
            ))
        } else {
            Arc::clone(&old.graphql_contracts)
        };
        let identifier_reference_resolutions = if plan.identifier_references {
            self.counters.reference_rebuilds += 1;
            let references = flatten_references(&self.references_by_path);
            Arc::new(resolve_identifier_references_with_options(
                &project,
                &references,
                &self.options,
            ))
        } else {
            Arc::clone(&old.identifier_reference_resolutions)
        };

        let affected_paths = affected_paths(
            &changed_paths,
            &old.uri_graph,
            &uri_graph,
            &project,
            global_invalidation,
            plan.has_dependency_impact(),
        );
        self.counters.generations += 1;
        let generation = old.generation + 1;
        self.snapshot = Arc::new(DartWorkspaceSnapshot {
            generation,
            project,
            uri_graph,
            part_links,
            graphql_contracts,
            identifier_reference_resolutions,
        });

        DartWorkspaceUpdate {
            generation,
            changed_paths: changed_paths.into_iter().collect(),
            affected_paths,
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
}

impl RebuildPlan {
    const fn all() -> Self {
        Self {
            project: true,
            uri_graph: true,
            part_links: true,
            graphql_contracts: true,
            identifier_references: true,
        }
    }

    const fn project_only() -> Self {
        Self {
            project: true,
            uri_graph: false,
            part_links: false,
            graphql_contracts: false,
            identifier_references: false,
        }
    }

    const fn metadata(resolution_changed: bool) -> Self {
        Self {
            project: true,
            uri_graph: resolution_changed,
            part_links: resolution_changed,
            graphql_contracts: resolution_changed,
            identifier_references: resolution_changed,
        }
    }

    const fn options() -> Self {
        Self {
            project: false,
            uri_graph: true,
            part_links: false,
            graphql_contracts: true,
            identifier_references: true,
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

    const fn has_dependency_impact(self) -> bool {
        self.uri_graph || self.part_links || self.graphql_contracts || self.identifier_references
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

    RebuildPlan {
        project: file_changed,
        uri_graph: import_export_changed || part_directives_changed,
        part_links: part_directives_changed || library_membership_changed,
        graphql_contracts: namespace_changed
            || old.graphql_operations != new.graphql_operations
            || old.graphql_operation_uses != new.graphql_operation_uses,
        identifier_references: namespace_changed
            || old.declarations != new.declarations
            || references_changed,
    }
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

fn flatten_references(
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
) -> Vec<DartIdentifierReference> {
    references_by_path
        .values()
        .flat_map(|references| references.iter().cloned())
        .collect()
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
