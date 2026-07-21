use dartscope_core::{DartDeclarationKind, DartFileInput, DartProjectInput};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolutionStatus, DartDefinitionTarget, DartWorkspaceIndex,
    DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

const SOURCE: &str = r#"
class Vector {
  Vector operator +(Vector other) => this;
  bool operator ==(Object other) => false;
  int operator <<(int amount) => 0;

  void exercise(Vector other) {
    final sum = this + other;
    final equal = this == other;
    final shifted = this << 1;
    final missing = this - other;
  }
}
"#;

#[test]
fn resolves_direct_operator_targets_and_reverse_references() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/vector.dart", SOURCE)],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let plus = invocation("this + other", "+");
    let equal = invocation("this == other", "==");
    let shift = invocation("this << 1", "<<");
    let missing = invocation("this - other", "-");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/vector.dart", plus),
        DartDefinitionQuery::new("lib/vector.dart", equal),
        DartDefinitionQuery::new("lib/vector.dart", shift),
        DartDefinitionQuery::new("lib/vector.dart", missing),
    ]);

    for (at, name) in [(plus, "+"), (equal, "=="), (shift, "<<")] {
        let resolution = batch
            .resolutions
            .iter()
            .find(|resolution| resolution.query.byte_offset == at)
            .expect("operator resolution");
        assert_eq!(resolution.status, DartDefinitionResolutionStatus::Resolved);
        assert!(matches!(
            &resolution.targets[0],
            DartDefinitionTarget::Namespace(candidate)
                if candidate.kind == DartDeclarationKind::Operator && candidate.name == name
        ));
    }

    let missing = batch
        .resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == missing)
        .expect("missing operator resolution");
    assert_eq!(missing.status, DartDefinitionResolutionStatus::Missing);
    assert!(matches!(
        &missing.targets[0],
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Class && candidate.name == "Vector"
    ));

    let plus_target = batch
        .resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == plus)
        .expect("plus resolution")
        .targets[0]
        .clone();
    let references = context.find_references(std::slice::from_ref(&plus_target));
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].references.len(), 1);
    assert_eq!(references.results[0].references[0].span.byte_start, plus);
}

const OWNER: &str = r#"
library sample;
part 'part.dart';
"#;
const PART: &str = r#"
part of 'owner.dart';

class NumberBox {
  NumberBox operator +(NumberBox other) => this;

  void exercise(NumberBox other) {
    final sum = this + other;
  }
}
"#;

#[test]
fn preserves_part_library_and_snapshot_operator_parity() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let query = DartDefinitionQuery::new("lib/part.dart", occurrence(PART, "this + other", "+"));
    let expected = DartWorkspaceResolutionContext::new(&analysis)
        .find_definitions(std::slice::from_ref(&query));
    assert_eq!(
        expected.resolutions[0].status,
        DartDefinitionResolutionStatus::Resolved
    );
    assert!(matches!(
        &expected.resolutions[0].targets[0],
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Operator
                && candidate.name == "+"
                && candidate.declaration_path == "lib/part.dart"
    ));

    let index = DartWorkspaceIndex::from_reference_project(analysis);
    let snapshot = index.snapshot();
    let actual =
        DartWorkspaceResolutionContext::from_snapshot(snapshot.as_ref()).find_definitions(&[query]);
    assert_eq!(actual, expected);
}

fn invocation(fragment: &str, token: &str) -> usize {
    occurrence(SOURCE, fragment, token)
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
