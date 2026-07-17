#!/usr/bin/env python3
"""Complete DS-INDEX-005 per-source reference invalidation semantics."""

from pathlib import Path

ROOT = Path(".")


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one anchor, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")


INCREMENTAL = "crates/dartscope-index/src/incremental.rs"
TESTS = "crates/dartscope-index/src/tests/incremental.rs"
ROADMAP = "docs/development/dartscope-library-plan.md"
DOC = "docs/development/incremental-index.md"
CHANGELOG = "CHANGELOG.md"

replace_once(
    INCREMENTAL,
    """    DartIdentifierReferenceResolutionAnalysis, DartPartLinkAnalysis, DartProjectAnalysis,
""",
    """    DartIdentifierReferenceResolutionAnalysis, DartPartLinkAnalysis, DartPartLinkStatus,
    DartProjectAnalysis,
""",
)

replace_once(
    INCREMENTAL,
    """        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed),
            None => RebuildPlan::all(),
        };
        self.files.insert(path.clone(), file);
""",
    """        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed),
            None => RebuildPlan::all(),
        };
        let changed_declaration_names =
            changed_top_level_declaration_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
""",
)

replace_once(
    INCREMENTAL,
    """        self.rebuild(plan, BTreeSet::from([path]), false, old_file.is_none())
""",
    """        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
        )
""",
)

replace_once(
    INCREMENTAL,
    """        if self.files.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.references_by_path.remove(&path);
        self.rebuild(RebuildPlan::all(), BTreeSet::from([path]), false, true)
""",
    """        let Some(removed) = self.files.remove(&path) else {
            return self.no_op_update();
        };
        let changed_declaration_names =
            changed_top_level_declaration_names(Some(&removed), None);
        self.references_by_path.remove(&path);
        self.rebuild(
            RebuildPlan::all(),
            BTreeSet::from([path]),
            false,
            true,
            changed_declaration_names,
        )
""",
)

replace_once(
    INCREMENTAL,
    """        self.rebuild(
            RebuildPlan::metadata(resolution_changed),
            BTreeSet::from([path]),
            resolution_changed,
            false,
        )
""",
    """        self.rebuild(
            RebuildPlan::metadata(resolution_changed),
            BTreeSet::from([path]),
            resolution_changed,
            false,
            BTreeSet::new(),
        )
""",
)

metadata_call = """        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
        )
"""
metadata_replacement = """        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
        )
"""
target = ROOT / INCREMENTAL
text = target.read_text(encoding="utf-8")
count = text.count(metadata_call)
if count != 3:
    raise SystemExit(f"{INCREMENTAL}: expected three metadata calls, found {count}")
target.write_text(text.replace(metadata_call, metadata_replacement), encoding="utf-8")

replace_once(
    INCREMENTAL,
    """        self.rebuild(RebuildPlan::options(), BTreeSet::new(), true, false)
""",
    """        self.rebuild(
            RebuildPlan::options(),
            BTreeSet::new(),
            true,
            false,
            BTreeSet::new(),
        )
""",
)

replace_once(
    INCREMENTAL,
    """        self.rebuild(RebuildPlan::project_only(), BTreeSet::new(), false, false)
""",
    """        self.rebuild(
            RebuildPlan::project_only(),
            BTreeSet::new(),
            false,
            false,
            BTreeSet::new(),
        )
""",
)

replace_once(
    INCREMENTAL,
    """        global_invalidation: bool,
        file_set_changed: bool,
    ) -> DartWorkspaceUpdate {
""",
    """        global_invalidation: bool,
        file_set_changed: bool,
        changed_declaration_names: BTreeSet<String>,
    ) -> DartWorkspaceUpdate {
""",
)

replace_once(
    INCREMENTAL,
    """        let affected_paths = affected_paths(
            &changed_paths,
            &old.uri_graph,
            &uri_graph,
            &project,
            global_invalidation,
            plan.propagate_dependents,
        );
        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links_with_graph(&project, &uri_graph))
        } else {
            Arc::clone(&old.part_links)
        };
""",
    """        let part_links = if plan.part_links {
            self.counters.part_link_rebuilds += 1;
            Arc::new(analyze_part_links_with_graph(&project, &uri_graph))
        } else {
            Arc::clone(&old.part_links)
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
""",
)

replace_once(
    INCREMENTAL,
    """    let declarations_changed = old.declarations != new.declarations;
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
""",
    """    let top_level_declarations_changed =
        top_level_declaration_facts(old) != top_level_declaration_facts(new);
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
""",
)

replace_once(
    INCREMENTAL,
    """        identifier_references: namespace_changed || declarations_changed || references_changed,
        propagate_dependents: namespace_changed
            || declarations_changed
            || graphql_operations_changed,
""",
    """        identifier_references: namespace_changed
            || top_level_declarations_changed
            || references_changed,
        propagate_dependents: namespace_changed
            || top_level_declarations_changed
            || graphql_operations_changed,
""",
)

replace_once(
    INCREMENTAL,
    """fn build_uri_reference_cache(
""",
    """fn top_level_declaration_facts(
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
        .filter_map(|(path, references)| {
            references
                .iter()
                .any(|reference| names.contains(&reference.name))
                .then(|| path.clone())
        })
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

fn build_uri_reference_cache(
""",
)

replace_once(
    TESTS,
    """#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
""",
    """#[test]
fn same_name_not_visible_evidence_rebuilds_without_an_import_edge() {
    let analysis = reference_project(&[
        ("lib/use.dart", "void useHidden() { Hidden(); }\n"),
        ("lib/hidden.dart", "class Hidden {}\n"),
        ("lib/other.dart", "class Other {}\n"),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    let initial_snapshot = index.snapshot();
    let initial = &initial_snapshot
        .identifier_reference_resolutions()
        .resolutions[0];
    assert_eq!(initial.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(initial.candidates.len(), 1);

    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/hidden.dart", "class Renamed {}\n"),
    ));

    assert_eq!(
        update.affected_paths,
        vec!["lib/hidden.dart".to_string(), "lib/use.dart".to_string()]
    );
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
    let baseline = reference_project(&[
        ("lib/use.dart", "void useHidden() { Hidden(); }\n"),
        ("lib/hidden.dart", "class Renamed {}\n"),
        ("lib/other.dart", "class Other {}\n"),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    let updated_snapshot = index.snapshot();
    let resolution = &updated_snapshot
        .identifier_reference_resolutions()
        .resolutions[0];
    assert_eq!(resolution.status, DartSymbolResolutionStatus::Missing);
    assert!(resolution.candidates.is_empty());
}

#[test]
fn part_membership_changes_rebuild_sibling_reference_sources() {
    let analysis = reference_project(&[
        (
            "lib/owner.dart",
            "part 'left.dart';\npart 'right.dart';\nclass Owner {}\n",
        ),
        (
            "lib/left.dart",
            "part of 'owner.dart';\nclass Shared {}\n",
        ),
        (
            "lib/right.dart",
            "part of 'owner.dart';\nvoid useShared() { Shared(); }\n",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    assert_eq!(
        index.snapshot().identifier_reference_resolutions().resolutions[0].status,
        DartSymbolResolutionStatus::Resolved
    );

    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new(
            "lib/left.dart",
            "part of 'different.dart';\nclass Shared {}\n",
        ),
    ));

    assert_eq!(
        update.affected_paths,
        vec![
            "lib/left.dart".to_string(),
            "lib/owner.dart".to_string(),
            "lib/right.dart".to_string(),
        ]
    );
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/owner.dart",
            "part 'left.dart';\npart 'right.dart';\nclass Owner {}\n",
        ),
        (
            "lib/left.dart",
            "part of 'different.dart';\nclass Shared {}\n",
        ),
        (
            "lib/right.dart",
            "part of 'owner.dart';\nvoid useShared() { Shared(); }\n",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    assert_eq!(
        index.snapshot().identifier_reference_resolutions().resolutions[0].status,
        DartSymbolResolutionStatus::NotVisible
    );
}

#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
""",
)

replace_once(
    ROADMAP,
    """8. **P1 fixed:** replacing parser reference facts without changing a file namespace previously
   invalidated every transitive importer; it now invalidates only that source path.

Remaining work:
""",
    """8. **P1 fixed:** replacing parser reference facts without changing a file namespace previously
   invalidated every transitive importer; it now invalidates only that source path.
9. **P1 fixed:** reverse URI edges alone missed `NotVisible` candidate evidence from unrelated
   same-name top-level declarations. Declaration changes now invalidate every reference source using an
   affected name.
10. **P1 fixed:** changing `part of` membership could change sibling-part visibility without a direct
    reverse URI edge to that sibling. Old/new matched part components now extend reference invalidation.
11. **P1 fixed:** the first part-component helper echoed a changed metadata path into public
    `affected_paths`; it now returns only newly reached Dart owner/part paths.

Remaining work:
""",
)

replace_once(
    DOC,
    """A local reference-fact replacement invalidates only its source path. File insertion/removal recomputes
that path plus direct URI sources whose previous target resolution may change. Namespace/declaration
changes still report the transitive reverse closure and recompute reference sources in that closure.
""",
    """A local reference-fact replacement invalidates only its source path. File insertion/removal recomputes
that path plus direct URI sources whose previous target resolution may change. Namespace changes report
the transitive reverse closure. Top-level declaration changes additionally invalidate every reference
source using an affected name because retained `NotVisible` evidence can change without an import edge.
Changes to part membership also traverse old and new matched owner/part components so sibling-part
visibility stays equivalent to a clean rebuild. Metadata paths themselves are not emitted as Dart
`affected_paths` by this component traversal.
""",
)

replace_once(
    DOC,
    """closure ordering, and a deterministic 64-step mixed update sequence.
""",
    """closure ordering, same-name `NotVisible` evidence outside the URI graph, sibling-part visibility,
and a deterministic 64-step mixed update sequence.
""",
)

replace_once(
    CHANGELOG,
    """  counters.

### Compatibility
""",
    """  counters.

### Fixed

- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
  visibility changes without leaking non-Dart metadata paths into `affected_paths`.

### Compatibility
""",
)

print("DS-INDEX-005 semantic invalidation correction applied")
