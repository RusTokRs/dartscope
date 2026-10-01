// Regression spec from the 2026-09-30 audit: is the incremental workspace snapshot identical to a stateless analysis after a formatting-only edit?
use dartscope_core::{DartFileInput, DartProjectInput, DartProjectReferenceAnalysis};
use dartscope_index::{
    DartIndexOptions, DartWorkspaceIndex, resolve_project_identifier_references_with_options,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

fn project(sources: &[(&str, &str)]) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        sources
            .iter()
            .map(|(p, s)| DartFileInput::new(*p, *s))
            .collect(),
        vec![],
    ))
}

#[test]
fn formatting_only_edit_keeps_dependent_resolution_evidence_in_sync() {
    let a = "import 'b.dart';\nvoid useB() { B(); }\n";
    let mut index = DartWorkspaceIndex::from_reference_project(project(&[
        ("lib/a.dart", a),
        ("lib/b.dart", "class B {}\n"),
    ]));
    let shifted = "\n\nclass B {}\n";
    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/b.dart", shifted),
    ));
    println!(
        "spec I1 affected_paths after shifting `class B` down two lines: {:?}",
        update.affected_paths
    );
    let fresh = project(&[("lib/a.dart", a), ("lib/b.dart", shifted)]);
    let expected =
        resolve_project_identifier_references_with_options(&fresh, &DartIndexOptions::default());
    let actual = index.snapshot().identifier_reference_resolutions().clone();
    assert_eq!(
        actual, expected,
        "incremental resolutions diverge from a stateless analysis (stale declaration spans in dependents)"
    );
}

#[test]
fn adding_a_member_keeps_dependent_resolution_evidence_in_sync() {
    let a = "import 'b.dart';\nvoid useB() { B(); }\n";
    let mut index = DartWorkspaceIndex::from_reference_project(project(&[
        ("lib/a.dart", a),
        ("lib/b.dart", "class B {}\n"),
    ]));
    let edited = "class B { int value = 1; }\n";
    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/b.dart", edited),
    ));
    println!(
        "spec I2 affected_paths after adding a member to B: {:?}",
        update.affected_paths
    );
    let fresh = project(&[("lib/a.dart", a), ("lib/b.dart", edited)]);
    let expected =
        resolve_project_identifier_references_with_options(&fresh, &DartIndexOptions::default());
    assert_eq!(
        index.snapshot().identifier_reference_resolutions(),
        &expected
    );
}

/// Compares what the index kept for a dependent after `lib/b.dart` was edited with a stateless analysis.
fn assert_dependent_in_sync(a: &str, before: &str, after: &str) {
    let mut index = DartWorkspaceIndex::from_reference_project(project(&[
        ("lib/a.dart", a),
        ("lib/b.dart", before),
    ]));
    let _ = index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
        "lib/b.dart",
        after,
    )));
    let fresh = project(&[("lib/a.dart", a), ("lib/b.dart", after)]);
    let expected =
        resolve_project_identifier_references_with_options(&fresh, &DartIndexOptions::default());
    assert_eq!(
        index.snapshot().identifier_reference_resolutions(),
        &expected,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn a_body_that_grows_below_the_first_line_keeps_dependent_spans_in_sync() {
    // The first line of `class B` does not change, so only its full declaration span moves. The
    // resolution cached for `lib/a.dart` carries that span.
    assert_dependent_in_sync(
        "import 'b.dart';\nvoid useB() { B(); }\n",
        "class B {\n}\n",
        "class B {\n  int value = 1;\n}\n",
    );
    assert_dependent_in_sync(
        "import 'b.dart';\nvoid useB() { B(); }\n",
        "class B {\n  int value = 1;\n  void run() {}\n}\n",
        "class B {\n  void run() {}\n}\n",
    );
}

#[test]
fn a_member_rename_of_the_same_length_invalidates_dependents() {
    // Nothing about the span of `class B` changes, but `this.a` no longer names a member of it.
    assert_dependent_in_sync(
        "import 'b.dart';\nclass C extends B {\n  void f() {\n    this.a;\n  }\n}\n",
        "class B {\n  int a = 1;\n}\n",
        "class B {\n  int b = 1;\n}\n",
    );
}

#[test]
fn a_changed_supertype_invalidates_dependents() {
    // `C` inherits `a` through `B`; once `B extends Other` instead of `Base`, the member is not found.
    assert_dependent_in_sync(
        "import 'b.dart';\nclass C extends B {\n  void f() {\n    this.a;\n  }\n}\n",
        "class Base {\n  int a = 1;\n}\nclass Other {}\nclass B extends Base {}\n",
        "class Base {\n  int a = 1;\n}\nclass Other {}\nclass B extends Other {}\n",
    );
}
