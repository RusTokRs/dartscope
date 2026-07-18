#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INCREMENTAL = ROOT / "crates/dartscope-index/src/incremental.rs"
LIB = ROOT / "crates/dartscope-index/src/lib.rs"
TESTS = ROOT / "crates/dartscope-index/src/tests/incremental.rs"
EXAMPLE = ROOT / "crates/dartscope-index/examples/incremental_workspace_baseline.rs"
DOC = ROOT / "docs/development/incremental-index.md"
ROADMAP = ROOT / "docs/development/dartscope-library-plan.md"
CHANGELOG = ROOT / "CHANGELOG.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    INCREMENTAL,
    """    DartProjectAnalysis, DartProjectReferenceAnalysis, DartProjectSummary, DartUriGraph,
    DartUriReference, PackageConfigAnalysis, PubspecAnalysis, normalize_path,
""",
    """    DartProjectAnalysis, DartProjectReferenceAnalysis, DartProjectSummary, DartUriGraph,
    DartUriReference, DartUriReferenceKind, PackageConfigAnalysis, PubspecAnalysis, normalize_path,
""",
)
replace_once(
    INCREMENTAL,
    """/// Immutable, shareable view of one workspace-index generation.
""",
    """/// Stable import/export dependency evidence for one normalized Dart library.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartLibraryDependencyFingerprint {
    pub owner_path: String,
    pub member_paths: Vec<String>,
    pub references: Vec<DartUriReference>,
}

/// Immutable, shareable view of one workspace-index generation.
""",
)
replace_once(
    INCREMENTAL,
    """    uri_graph: Arc<DartUriGraph>,
    part_links: Arc<DartPartLinkAnalysis>,
    graphql_contracts: Arc<DartGraphqlContractAnalysis>,
""",
    """    uri_graph: Arc<DartUriGraph>,
    part_links: Arc<DartPartLinkAnalysis>,
    library_dependency_fingerprints: Arc<Vec<DartLibraryDependencyFingerprint>>,
    graphql_contracts: Arc<DartGraphqlContractAnalysis>,
""",
)
replace_once(
    INCREMENTAL,
    """    pub fn part_links(&self) -> &DartPartLinkAnalysis {
        &self.part_links
    }

    pub fn graphql_contracts(&self) -> &DartGraphqlContractAnalysis {
""",
    """    pub fn part_links(&self) -> &DartPartLinkAnalysis {
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
""",
)
replace_once(
    INCREMENTAL,
    """    pub part_link_rebuilds: u64,
    pub namespace_libraries_rebuilt: u64,
    pub graphql_rebuilds: u64,
""",
    """    pub part_link_rebuilds: u64,
    pub namespace_libraries_rebuilt: u64,
    pub library_dependency_fingerprints_rebuilt: u64,
    pub graphql_rebuilds: u64,
""",
)
replace_once(
    INCREMENTAL,
    """    pub changed_paths: Vec<String>,
    pub affected_paths: Vec<String>,
    pub rebuilt: DartWorkspaceSubsystems,
""",
    """    pub changed_paths: Vec<String>,
    pub affected_paths: Vec<String>,
    pub affected_libraries: Vec<String>,
    pub rebuilt: DartWorkspaceSubsystems,
""",
)
replace_once(
    INCREMENTAL,
    """    uri_references_by_path: BTreeMap<String, Arc<Vec<DartUriReference>>>,
    library_paths_by_owner: BTreeMap<String, Arc<Vec<String>>>,
    graphql_contracts_by_library: BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
""",
    """    uri_references_by_path: BTreeMap<String, Arc<Vec<DartUriReference>>>,
    library_paths_by_owner: BTreeMap<String, Arc<Vec<String>>>,
    library_dependency_fingerprints_by_owner:
        BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
    graphql_contracts_by_library: BTreeMap<String, Arc<DartGraphqlContractAnalysis>>,
""",
)
replace_once(
    INCREMENTAL,
    """        let part_links = Arc::new(analyze_part_links_with_graph(&project, &uri_graph));
        let library_paths_by_owner = build_library_path_cache(&project, &part_links);
        let (graphql_contracts_by_library, graphql_contracts) = build_graphql_contract_cache(
""",
    """        let part_links = Arc::new(analyze_part_links_with_graph(&project, &uri_graph));
        let library_paths_by_owner = build_library_path_cache(&project, &part_links);
        let library_dependency_fingerprints_by_owner =
            build_library_dependency_fingerprint_cache(&uri_graph, &library_paths_by_owner);
        let library_dependency_fingerprints = Arc::new(aggregate_library_dependency_fingerprints(
            &library_dependency_fingerprints_by_owner,
        ));
        let (graphql_contracts_by_library, graphql_contracts) = build_graphql_contract_cache(
""",
)
replace_once(
    INCREMENTAL,
    """        let initial_uri_files = uri_references_by_path.len() as u64;
        let initial_namespace_libraries = library_paths_by_owner.len() as u64;
        let initial_graphql_libraries = graphql_contracts_by_library.len() as u64;
""",
    """        let initial_uri_files = uri_references_by_path.len() as u64;
        let initial_namespace_libraries = library_paths_by_owner.len() as u64;
        let initial_dependency_fingerprints = library_dependency_fingerprints_by_owner.len() as u64;
        let initial_graphql_libraries = graphql_contracts_by_library.len() as u64;
""",
)
replace_once(
    INCREMENTAL,
    """            uri_graph,
            part_links,
            graphql_contracts,
""",
    """            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
""",
)
replace_once(
    INCREMENTAL,
    """            uri_references_by_path,
            library_paths_by_owner,
            graphql_contracts_by_library,
""",
    """            uri_references_by_path,
            library_paths_by_owner,
            library_dependency_fingerprints_by_owner,
            graphql_contracts_by_library,
""",
)
replace_once(
    INCREMENTAL,
    """                part_link_rebuilds: 1,
                namespace_libraries_rebuilt: initial_namespace_libraries,
                graphql_rebuilds: 1,
""",
    """                part_link_rebuilds: 1,
                namespace_libraries_rebuilt: initial_namespace_libraries,
                library_dependency_fingerprints_rebuilt: initial_dependency_fingerprints,
                graphql_rebuilds: 1,
""",
)
replace_once(
    INCREMENTAL,
    """            changed_paths: Vec::new(),
            affected_paths: Vec::new(),
            rebuilt: DartWorkspaceSubsystems::default(),
""",
    """            changed_paths: Vec::new(),
            affected_paths: Vec::new(),
            affected_libraries: Vec::new(),
            rebuilt: DartWorkspaceSubsystems::default(),
""",
)
replace_once(
    INCREMENTAL,
    """        if plan.part_links || file_set_changed {
            self.counters.namespace_libraries_rebuilt +=
                refresh_library_path_cache(&project, &part_links, &mut self.library_paths_by_owner);
        }
        let mut affected_paths: BTreeSet<_> = affected_paths(
""",
    """        if plan.part_links || file_set_changed {
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
""",
)
replace_once(
    INCREMENTAL,
    """        let affected_paths: Vec<_> = affected_paths.into_iter().collect();
        let graphql_contracts = if plan.graphql_contracts {
""",
    """        let affected_paths: Vec<_> = affected_paths.into_iter().collect();
        let affected_libraries = affected_library_owners(
            &changed_paths,
            &affected_paths,
            &old.project,
            &old.part_links,
            &project,
            &part_links,
        );
        let graphql_contracts = if plan.graphql_contracts {
""",
)
replace_once(
    INCREMENTAL,
    """            uri_graph,
            part_links,
            graphql_contracts,
            identifier_reference_resolutions,
""",
    """            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
            identifier_reference_resolutions,
""",
)
replace_once(
    INCREMENTAL,
    """            changed_paths: changed_paths.into_iter().collect(),
            affected_paths,
            rebuilt: plan.public(),
""",
    """            changed_paths: changed_paths.into_iter().collect(),
            affected_paths,
            affected_libraries,
            rebuilt: plan.public(),
""",
)
replace_once(
    INCREMENTAL,
    """fn graphql_library_owners(
""",
    """fn build_library_dependency_fingerprint_cache(
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
    let old_files: BTreeSet<_> = old_project.files.iter().map(|file| file.path.as_str()).collect();
    let new_files: BTreeSet<_> = new_project.files.iter().map(|file| file.path.as_str()).collect();
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
""",
)

replace_once(
    LIB,
    """pub use incremental::{
    DartWorkspaceIndex, DartWorkspaceIndexCounters, DartWorkspaceSnapshot, DartWorkspaceSubsystems,
    DartWorkspaceUpdate,
};
""",
    """pub use incremental::{
    DartLibraryDependencyFingerprint, DartWorkspaceIndex, DartWorkspaceIndexCounters,
    DartWorkspaceSnapshot, DartWorkspaceSubsystems, DartWorkspaceUpdate,
};
""",
)

replace_once(
    TESTS,
    """fn reference_project(sources: &[(&str, &str)]) -> DartProjectReferenceAnalysis {
""",
    """#[test]
fn dependency_fingerprints_track_resolution_and_retained_snapshots() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/client.dart", "import 'missing.dart';\n"),
            DartFileInput::new("lib/stable.dart", "class Stable {}\n"),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let retained = index.snapshot();
    let before = index.counters();
    assert_eq!(
        retained
            .library_dependency_fingerprint("lib/client.dart")
            .expect("client library fingerprint")
            .references[0]
            .resolution,
        DartUriResolution::MissingTarget
    );

    let update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/missing.dart",
        "class Missing {}\n",
    )));

    assert_eq!(
        update.affected_libraries,
        vec!["lib/client.dart".to_string(), "lib/missing.dart".to_string()]
    );
    assert_eq!(
        index.counters().library_dependency_fingerprints_rebuilt,
        before.library_dependency_fingerprints_rebuilt + 2
    );
    assert_eq!(
        retained
            .library_dependency_fingerprint("lib/client.dart")
            .expect("retained client fingerprint")
            .references[0]
            .resolution,
        DartUriResolution::MissingTarget
    );
    assert_eq!(
        index
            .snapshot()
            .library_dependency_fingerprint("lib/client.dart")
            .expect("updated client fingerprint")
            .references[0]
            .resolution,
        DartUriResolution::Resolved
    );
    assert_snapshot_matches_project_only(&index, &DartIndexOptions::default());
}

#[test]
fn dependency_fingerprints_rebuild_only_the_changed_library() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/a.dart", "import 'b.dart';\n"),
            DartFileInput::new("lib/b.dart", "class B {}\n"),
            DartFileInput::new("lib/c.dart", "class C {}\n"),
            DartFileInput::new("lib/x.dart", "import 'y.dart';\n"),
            DartFileInput::new("lib/y.dart", "class Y {}\n"),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let before = index.counters();
    let x_before = index
        .snapshot()
        .library_dependency_fingerprint("lib/x.dart")
        .expect("x library fingerprint")
        .clone();

    let update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/a.dart",
        "import 'c.dart';\n",
    )));

    assert_eq!(update.affected_libraries, vec!["lib/a.dart".to_string()]);
    assert_eq!(
        index.counters().library_dependency_fingerprints_rebuilt,
        before.library_dependency_fingerprints_rebuilt + 1
    );
    let snapshot = index.snapshot();
    assert_eq!(
        snapshot
            .library_dependency_fingerprint("lib/x.dart")
            .expect("retained x fingerprint"),
        &x_before
    );
    assert_eq!(
        snapshot
            .library_dependency_fingerprint("lib/a.dart")
            .expect("updated a fingerprint")
            .references[0]
            .target_path
            .as_deref(),
        Some("lib/c.dart")
    );
    assert_snapshot_matches_project_only(&index, &DartIndexOptions::default());
}

#[test]
fn affected_libraries_collapse_part_membership_to_the_owner() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", "part 'child.dart';\n"),
            DartFileInput::new(
                "lib/child.dart",
                "part of 'owner.dart';\nclass Child {}\n",
            ),
            DartFileInput::new("lib/independent.dart", "class Independent {}\n"),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let before = index.counters();

    let update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/child.dart",
        "part of 'owner.dart';\nclass RenamedChild {}\n",
    )));

    assert_eq!(
        update.affected_libraries,
        vec!["lib/owner.dart".to_string()]
    );
    assert_eq!(
        index.counters().library_dependency_fingerprints_rebuilt,
        before.library_dependency_fingerprints_rebuilt
    );
    assert_snapshot_matches_project_only(&index, &DartIndexOptions::default());
}

fn reference_project(sources: &[(&str, &str)]) -> DartProjectReferenceAnalysis {
""",
)
replace_once(
    TESTS,
    """    assert_eq!(
        snapshot.identifier_reference_resolutions(),
        &resolve_project_identifier_references_with_options(baseline, options)
    );
}
""",
    """    assert_eq!(
        snapshot.identifier_reference_resolutions(),
        &resolve_project_identifier_references_with_options(baseline, options)
    );
    let fresh = DartWorkspaceIndex::from_reference_project_with_options(
        baseline.clone(),
        options.clone(),
    );
    let fresh_snapshot = fresh.snapshot();
    assert_eq!(
        snapshot.library_dependency_fingerprints(),
        fresh_snapshot.library_dependency_fingerprints()
    );
}
""",
)
replace_once(
    TESTS,
    """    assert_eq!(
        snapshot.graphql_contracts(),
        &analyze_graphql_contracts_with_options(snapshot.project(), options)
    );
}
""",
    """    assert_eq!(
        snapshot.graphql_contracts(),
        &analyze_graphql_contracts_with_options(snapshot.project(), options)
    );
    let fresh = DartWorkspaceIndex::from_project_with_options(
        snapshot.project().clone(),
        options.clone(),
    );
    let fresh_snapshot = fresh.snapshot();
    assert_eq!(
        snapshot.library_dependency_fingerprints(),
        fresh_snapshot.library_dependency_fingerprints()
    );
}
""",
)

replace_once(
    EXAMPLE,
    """        assert_eq!(counters.namespace_libraries_rebuilt, file_count as u64);
        assert_eq!(counters.graphql_libraries_rebuilt, 0);
""",
    """        assert_eq!(counters.namespace_libraries_rebuilt, file_count as u64);
        assert_eq!(
            counters.library_dependency_fingerprints_rebuilt,
            file_count as u64
        );
        assert_eq!(counters.graphql_libraries_rebuilt, 0);
""",
)

replace_once(
    DOC,
    """- part links;
- GraphQL operation bindings;
""",
    """- part links;
- stable per-library import/export dependency fingerprints;
- GraphQL operation bindings;
""",
)
replace_once(
    DOC,
    """`DartWorkspaceUpdate` reports normalized changed paths, the transitive reverse dependency closure, and
which products were rebuilt. Reverse dependencies include resolved targets, missing target paths, and
ambiguous package candidates from both the old and new URI graph.
""",
    """`DartWorkspaceUpdate` reports normalized changed paths, the transitive reverse dependency closure,
normalized affected library owners, and which products were rebuilt. Reverse dependencies include
resolved targets, missing target paths, and ambiguous package candidates from both the old and new URI
graph. Part paths collapse to their matched owner in `affected_libraries`; metadata paths are excluded.
""",
)
replace_once(
    DOC,
    """`DartWorkspaceIndexCounters` records generations, aggregate rebuilds, the exact number of URI and
identifier-reference source files recomputed, and the number of namespace-membership and GraphQL-use
libraries refreshed. Unaffected per-file and per-library `Arc` cache entries remain shared internally.
""",
    """`DartWorkspaceIndexCounters` records generations, aggregate rebuilds, the exact number of URI and
identifier-reference source files recomputed, and the number of namespace-membership, dependency-
fingerprint, and GraphQL-use libraries refreshed. Unaffected per-file and per-library `Arc` cache entries
remain shared internally.
""",
)
replace_once(
    DOC,
    """URI references and identifier-reference resolutions use per-source-file caches. Library membership
and GraphQL bindings use retained per-library caches, while public snapshots retain the same aggregate
models. A later DS-INDEX-005 slice will persist import/export dependency fingerprints and expose the same
affected-library evidence to lint contexts. The public stateful API and existing stateless APIs remain
stable while that internal granularity improves.
""",
    """URI references and identifier-reference resolutions use per-source-file caches. Library membership,
import/export dependency fingerprints, and GraphQL bindings use retained per-library caches. Snapshots
publish deterministic fingerprints without exposing mutable cache storage, and updates publish the same
normalized affected-library owners that the next lint-context slice will consume. The public stateless
APIs remain available.
""",
)

replace_once(
    ROADMAP,
    """12. Added retained per-library namespace-membership and GraphQL-binding caches. GraphQL operation
    changes rebuild only libraries with affected uses, including unrelated `NotVisible` evidence and
    sibling parts, while the public aggregate snapshot remains unchanged.

Remaining work:

1. Add persistent per-library import/export dependency fingerprints for lint-context reuse beyond the
   completed membership and GraphQL binding caches.
2. Feed the same affected-library evidence into lint contexts without introducing an index/lint
   dependency cycle.
3. Add memory/update-time baselines for the per-library cache implementation.
""",
    """12. Added retained per-library namespace-membership and GraphQL-binding caches. GraphQL operation
    changes rebuild only libraries with affected uses, including unrelated `NotVisible` evidence and
    sibling parts, while the public aggregate snapshot remains unchanged.
13. Added retained per-library import/export dependency fingerprints and deterministic affected-library
    owners on every workspace update. Fingerprints preserve exact URI-resolution evidence while unchanged
    library entries remain shared across generations.

Remaining work:

1. Feed the same affected-library evidence into lint contexts without introducing an index/lint
   dependency cycle.
2. Add memory/update-time baselines for the per-library cache implementation.
""",
)

replace_once(
    CHANGELOG,
    """- A stateful workspace index foundation with normalized file/configuration mutations, immutable shared
  snapshots, deterministic reverse invalidation evidence, per-source URI/reference caches, and operation
  counters.
""",
    """- A stateful workspace index foundation with normalized file/configuration mutations, immutable shared
  snapshots, deterministic reverse invalidation evidence, per-source URI/reference caches, and operation
  counters.
- Persistent per-library import/export dependency fingerprints with deterministic affected-library
  evidence for downstream incremental consumers.
""",
)

print("DS-INDEX-005 dependency fingerprint slice applied")
