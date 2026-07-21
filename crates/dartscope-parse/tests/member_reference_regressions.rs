use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
class _Service {
  int first = 0, second = first;

  void repeat(int repeat) {}

  set value(int value) {}

  static void run() {}
  static int count = 0;
}

void use() {
  _Service.run();
  final count = _Service.count;
}
"#;

#[test]
fn declaration_facts_anchor_the_declared_member_name() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    assert_reference_start(
        &analysis.references,
        "repeat",
        DartIdentifierReferenceKind::MemberDeclarationInstance,
        SOURCE.find("repeat(int").expect("method declaration"),
    );
    assert_reference_start(
        &analysis.references,
        "value",
        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
        SOURCE.find("value(int").expect("setter declaration"),
    );
    assert_reference_start(
        &analysis.references,
        "first",
        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
        SOURCE.find("first = 0").expect("first field declaration"),
    );
    assert_reference_start(
        &analysis.references,
        "second",
        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
        SOURCE.find("second = first").expect("second field declaration"),
    );
}

#[test]
fn private_named_types_emit_static_member_facts() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    assert_reference_start(
        &analysis.references,
        "run",
        DartIdentifierReferenceKind::MemberInvocationStatic,
        occurrence("_Service.run", "run"),
    );
    assert_reference_start(
        &analysis.references,
        "count",
        DartIdentifierReferenceKind::MemberPropertyReadStatic,
        occurrence("_Service.count;", "count"),
    );
}

fn assert_reference_start(
    references: &[DartIdentifierReference],
    name: &str,
    kind: DartIdentifierReferenceKind,
    expected_start: usize,
) {
    let reference = references
        .iter()
        .find(|reference| reference.name == name && reference.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} reference for {name}"));
    assert_eq!(reference.span.byte_start, expected_start);
    assert_eq!(
        &SOURCE[reference.span.byte_start..reference.span.byte_end],
        name
    );
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start + SOURCE[start..start + fragment.len()].find(token).expect("token")
}
