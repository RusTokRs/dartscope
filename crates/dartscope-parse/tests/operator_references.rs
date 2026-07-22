use dartscope_core::{DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
class Vector {
  Vector operator +(Vector other) => this;
  bool operator ==(Object other) => false;
  int operator <<(int amount) => 0;
  Vector operator -() => this;
  int operator ~() => 0;
  int operator [](int index) => index;
  void operator []=(int index, int value) {}

  void exercise(Vector other) {
    final sum = this + other;
    final equal = this == other;
    final shifted = this << 1;
    final negated = -this;
    final inverted = ~this;
    final indexed = this[0];
    this[0] = 1;
    final notDirect = other + this + other;
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

    assert_eq!(operators.len(), 14);
    for operator in ["+", "==", "<<", "-", "~", "[]", "[]="] {
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
        let expected_invocation_anchor = if matches!(operator, "[]" | "[]=") {
            "["
        } else {
            operator
        };
        assert_eq!(
            &SOURCE[invocation.span.byte_start..invocation.span.byte_end],
            expected_invocation_anchor
        );
        assert_eq!(declaration.prefix, invocation.prefix);
    }

    assert_eq!(
        operators
            .iter()
            .filter(|reference| {
                reference.name == "+"
                    && reference.kind
                        == DartIdentifierReferenceKind::MemberOperatorInvocationInstance
            })
            .count(),
        1
    );
}
