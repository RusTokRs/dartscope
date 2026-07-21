use dartscope_core::{DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
class Vector {
  Vector operator +(Vector other) => this;
  bool operator ==(Object other) => false;
  int operator <<(int amount) => 0;

  void exercise(Vector other) {
    final sum = this + other;
    final equal = this == other;
    final shifted = this << 1;
  }
}
"#;

#[test]
fn emits_exact_operator_declaration_and_invocation_facts() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/vector.dart", SOURCE));
    let operators = analysis
        .references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::MemberOperatorDeclaration
                    | DartIdentifierReferenceKind::MemberOperatorInvocationInstance
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(operators.len(), 6);
    for operator in ["+", "==", "<<"] {
        let declaration = operators
            .iter()
            .find(|reference| {
                reference.name == operator
                    && reference.kind == DartIdentifierReferenceKind::MemberOperatorDeclaration
            })
            .unwrap_or_else(|| panic!("missing declaration fact for {operator}"));
        let invocation = operators
            .iter()
            .find(|reference| {
                reference.name == operator
                    && reference.kind
                        == DartIdentifierReferenceKind::MemberOperatorInvocationInstance
            })
            .unwrap_or_else(|| panic!("missing invocation fact for {operator}"));
        assert_eq!(
            &SOURCE[declaration.span.byte_start..declaration.span.byte_end],
            operator
        );
        assert_eq!(
            &SOURCE[invocation.span.byte_start..invocation.span.byte_end],
            operator
        );
        assert_eq!(declaration.prefix, invocation.prefix);
    }
}
