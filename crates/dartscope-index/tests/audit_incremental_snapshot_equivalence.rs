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
