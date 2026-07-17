#!/usr/bin/env python3
from pathlib import Path

ROOT = Path('.')

def replace_once(path, old, new):
    p = ROOT / path
    text = p.read_text(encoding='utf-8')
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{path}: expected one anchor, found {count}')
    p.write_text(text.replace(old, new), encoding='utf-8')

path = 'crates/dartscope-index/src/incremental.rs'

replace_once(path, '''        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed),
            None => RebuildPlan::all(),
        };
        self.files.insert(path.clone(), file);
''', '''        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed),
            None => RebuildPlan::all(),
        };
        let changed_declaration_names =
            changed_top_level_declaration_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
''')

replace_once(path, '''        self.rebuild(plan, BTreeSet::from([path]), false, old_file.is_none())
''', '''        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
        )
''')

replace_once(path, '''        if self.files.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.references_by_path.remove(&path);
        self.rebuild(RebuildPlan::all(), BTreeSet::from([path]), false, true)
''', '''        let Some(removed) = self.files.remove(&path) else {
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
''')

# Add an empty declaration-name set to every non-file mutation.
text = (ROOT / path).read_text(encoding='utf-8')
patterns = {
'''            resolution_changed,
            false,
        )
''': '''            resolution_changed,
            false,
            BTreeSet::new(),
        )
''',
'''            true,
            false,
        )
''': '''            true,
            false,
            BTreeSet::new(),
        )
''',
'''        self.rebuild(RebuildPlan::options(), BTreeSet::new(), true, false)
''': '''        self.rebuild(
            RebuildPlan::options(),
            BTreeSet::new(),
            true,
            false,
            BTreeSet::new(),
        )
''',
'''        self.rebuild(RebuildPlan::project_only(), BTreeSet::new(), false, false)
''': '''        self.rebuild(
            RebuildPlan::project_only(),
            BTreeSet::new(),
            false,
            false,
            BTreeSet::new(),
        )
''',
}
# First pattern occurs once; second occurs three times (metadata removal/upsert), handle explicitly.
old, new = list(patterns.items())[0]
if text.count(old) != 1:
    raise SystemExit(f'{path}: metadata upsert anchor count {text.count(old)}')
text = text.replace(old, new)
old, new = list(patterns.items())[1]
if text.count(old) != 3:
    raise SystemExit(f'{path}: metadata rebuild anchor count {text.count(old)}')
text = text.replace(old, new)
for old, new in list(patterns.items())[2:]:
    if text.count(old) != 1:
        raise SystemExit(f'{path}: rebuild call anchor count {text.count(old)}')
    text = text.replace(old, new)
(ROOT / path).write_text(text, encoding='utf-8')

replace_once(path, '''        global_invalidation: bool,
        file_set_changed: bool,
    ) -> DartWorkspaceUpdate {
''', '''        global_invalidation: bool,
        file_set_changed: bool,
        changed_declaration_names: BTreeSet<String>,
    ) -> DartWorkspaceUpdate {
''')

replace_once(path, '''        let affected_paths = affected_paths(
            &changed_paths,
            &old.uri_graph,
            &uri_graph,
            &project,
            global_invalidation,
            plan.propagate_dependents,
        );
''', '''        let mut affected_paths: BTreeSet<_> = affected_paths(
            &changed_paths,
            &old.uri_graph,
            &uri_graph,
            &project,
            global_invalidation,
            plan.propagate_dependents,
        )
        .into_iter()
        .collect();
        affected_paths.extend(reference_sources_for_declaration_names(
            &self.references_by_path,
            &changed_declaration_names,
        ));
        let affected_paths: Vec<_> = affected_paths.into_iter().collect();
''')

replace_once(path, '''    let declarations_changed = old.declarations != new.declarations;
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
''', '''    let top_level_declarations_changed =
        top_level_declaration_facts(old) != top_level_declaration_facts(new);
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
''')
replace_once(path, '''        identifier_references: namespace_changed || declarations_changed || references_changed,
        propagate_dependents: namespace_changed
            || declarations_changed
            || graphql_operations_changed,
''', '''        identifier_references: namespace_changed
            || top_level_declarations_changed
            || references_changed,
        propagate_dependents: namespace_changed
            || top_level_declarations_changed
            || graphql_operations_changed,
''')

replace_once(path, '''fn build_uri_reference_cache(
''', '''fn top_level_declaration_facts(
    file: &DartFileAnalysis,
) -> Vec<&dartscope_core::DartDeclaration> {
    file.declarations
        .iter()
        .filter(|declaration| declaration.parent_symbol_id.is_none())
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
    old_facts
        .into_iter()
        .chain(new_facts)
        .map(|declaration| declaration.name.clone())
        .collect()
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

fn build_uri_reference_cache(
''')

# Add a focused semantic regression test before randomized coverage.
test_path = 'crates/dartscope-index/src/tests/incremental.rs'
replace_once(test_path, '''#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
''', '''#[test]
fn same_name_not_visible_evidence_rebuilds_without_an_import_edge() {
    let analysis = reference_project(&[
        ("lib/use.dart", "void useHidden() { Hidden(); }\n"),
        ("lib/hidden.dart", "class Hidden {}\n"),
        ("lib/other.dart", "class Other {}\n"),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    let initial = &index
        .snapshot()
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
    let resolution = &index
        .snapshot()
        .identifier_reference_resolutions()
        .resolutions[0];
    assert_eq!(resolution.status, DartSymbolResolutionStatus::Missing);
    assert!(resolution.candidates.is_empty());
}

#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
''')

# Roadmap finding.
roadmap = 'docs/development/dartscope-library-plan.md'
replace_once(roadmap, '''8. **P1 fixed:** replacing parser reference facts without changing a file namespace previously
   invalidated every transitive importer; it now invalidates only that source path.

Remaining work:
''', '''8. **P1 fixed:** replacing parser reference facts without changing a file namespace previously
   invalidated every transitive importer; it now invalidates only that source path.
9. **P1 fixed:** the first per-file reference cache invalidated only reverse URI dependents, but
   `NotVisible` evidence also changes when an unrelated same-name top-level declaration changes. The
   cache now invalidates every reference source that uses an affected declaration name.

Remaining work:
''')

# Development contract.
doc = 'docs/development/incremental-index.md'
replace_once(doc, '''A local reference-fact replacement invalidates only its source path. File insertion/removal recomputes
that path plus direct URI sources whose previous target resolution may change. Namespace/declaration
changes still report the transitive reverse closure and recompute reference sources in that closure.
''', '''A local reference-fact replacement invalidates only its source path. File insertion/removal recomputes
that path plus direct URI sources whose previous target resolution may change. Namespace changes report
the transitive reverse closure. Top-level declaration changes additionally invalidate every reference
source using an affected declaration name, because retained `NotVisible` candidate evidence can change
even without an import edge.
''')
replace_once(doc, '''closure ordering, and a deterministic 64-step mixed update sequence.
''', '''closure ordering, same-name `NotVisible` evidence outside the URI graph, and a deterministic 64-step
mixed update sequence.
''')

print('DS-INDEX-005 symbol-name invalidation correction applied')
