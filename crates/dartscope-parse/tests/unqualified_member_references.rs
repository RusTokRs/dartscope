use dartscope_core::{DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
class Service {
  int count = 0;
  static int total = 0;

  void work() {}
  static void build() {}

  void exercise() {
    work();
    count = 2;
    count += 3;
    final read = count;
    build();
    total = 5;
    absent();
  }

  void shadowed(int count) {
    count = 1;
    final local = count;
    void work() {}
    work();
  }

  void localScope() {
    int work() => 3;
    work();
  }

  void lexicalShadow() {
    final work = 1;
    work();
  }
}

class Other {
  void run() {
    work();
  }

  void work() {}
}

void topLevel() {
  work();
}
"#;

#[test]
fn unqualified_same_owner_members_emit_exact_owner_facts() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));
    let owner = "lib/sample.dart::class:Service";

    assert_reference(
        &analysis.references,
        "work",
        DartIdentifierReferenceKind::MemberInvocationInstance,
        occurrence("work();\n    count = 2;", "work"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "count",
        DartIdentifierReferenceKind::MemberPropertyWriteInstance,
        occurrence("count = 2;", "count"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "count",
        DartIdentifierReferenceKind::MemberPropertyReadInstance,
        occurrence("count += 3;", "count"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "count",
        DartIdentifierReferenceKind::MemberPropertyWriteInstance,
        occurrence("count += 3;", "count"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "count",
        DartIdentifierReferenceKind::MemberPropertyReadInstance,
        occurrence("final read = count;", "count"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "build",
        DartIdentifierReferenceKind::MemberInvocationStatic,
        occurrence("build();", "build"),
        Some(owner),
    );
    assert_reference(
        &analysis.references,
        "total",
        DartIdentifierReferenceKind::MemberPropertyWriteStatic,
        occurrence("total = 5;", "total"),
        Some(owner),
    );
}

#[test]
fn unqualified_spellings_without_a_direct_member_keep_invocation_behavior() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    assert_reference(
        &analysis.references,
        "absent",
        DartIdentifierReferenceKind::InvocationTarget,
        occurrence("absent();", "absent"),
        None,
    );
    assert_reference(
        &analysis.references,
        "work",
        DartIdentifierReferenceKind::InvocationTarget,
        occurrence("void topLevel() {\n  work();", "work"),
        None,
    );
}

#[test]
fn lexical_shadowing_suppresses_unqualified_member_facts() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    for (source, name) in [
        ("count = 1;", "count"),
        ("final local = count;", "count"),
        ("void work() {}\n    work();", "work"),
        ("int work() => 3;\n    work();", "work"),
        ("final work = 1;\n    work();", "work"),
    ] {
        let start = occurrence(source, name);
        assert!(
            !analysis.references.iter().any(|reference| {
                is_member_kind(reference.kind) && reference.span.byte_start == start
            }),
            "member fact fabricated for {name:?} at {source:?}"
        );
    }
}

#[test]
fn enclosing_owner_is_not_borrowed_from_a_sibling_type() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    assert_reference(
        &analysis.references,
        "work",
        DartIdentifierReferenceKind::MemberInvocationInstance,
        occurrence("void run() {\n    work();", "work"),
        Some("lib/sample.dart::class:Other"),
    );
}

fn is_member_kind(kind: DartIdentifierReferenceKind) -> bool {
    matches!(
        kind,
        DartIdentifierReferenceKind::MemberDeclarationInstance
            | DartIdentifierReferenceKind::MemberDeclarationStatic
            | DartIdentifierReferenceKind::MemberInvocationInstance
            | DartIdentifierReferenceKind::MemberInvocationStatic
            | DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
            | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
            | DartIdentifierReferenceKind::MemberPropertyReadInstance
            | DartIdentifierReferenceKind::MemberPropertyReadStatic
            | DartIdentifierReferenceKind::MemberPropertyWriteInstance
            | DartIdentifierReferenceKind::MemberPropertyWriteStatic
            | DartIdentifierReferenceKind::MemberOperatorDeclaration
            | DartIdentifierReferenceKind::MemberOperatorInvocationInstance
    )
}

fn assert_reference(
    references: &[DartIdentifierReference],
    name: &str,
    kind: DartIdentifierReferenceKind,
    expected_start: usize,
    prefix: Option<&str>,
) {
    let reference = references
        .iter()
        .find(|reference| {
            reference.name == name
                && reference.kind == kind
                && reference.span.byte_start == expected_start
        })
        .unwrap_or_else(|| {
            panic!("missing {kind:?} reference {name:?} at byte {expected_start}");
        });
    assert_eq!(reference.prefix.as_deref(), prefix);
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
