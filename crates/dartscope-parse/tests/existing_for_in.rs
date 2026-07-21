use dartscope_core::{
    Confidence, DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? input) {}
Iterable<int> choose(Iterable<int> values, Object? current) => values;

void run(int value, Iterable<int> values) {
  for (value in choose(values, value)) {
    consume(value);
    value = value + 1;
  }
  {
    var value = 0;
    for (value in values) {
      value += 1;
      value();
    }
  }
  consume(value);
  for (final declared in values) {
    consume(declared);
  }
  for (value in values) value++;
}
"#;

#[test]
fn emits_normative_existing_for_in_writes_and_independent_reads() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/existing_for_in.dart", SOURCE));

    let parameter_target = occurrence("for (value in choose", "value");
    assert_eq!(
        variable_kinds_at(&analysis.references, parameter_target),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    let target = variable_references_at(&analysis.references, parameter_target);
    assert_eq!(target.len(), 1);
    assert_eq!(target[0].confidence, Confidence::High);
    assert_eq!(
        &SOURCE[target[0].span.byte_start..target[0].span.byte_end],
        "value"
    );
    assert!(
        target[0]
            .enclosing_symbol_id
            .as_deref()
            .is_some_and(|symbol_id| symbol_id.ends_with("::function:run"))
    );

    for offset in [
        occurrence("choose(values, value)", "values"),
        occurrence("choose(values, value)", ", value") + 2,
        occurrence("consume(value);\n    value = value + 1", "value"),
        occurrence("value = value + 1", "value + 1"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }

    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            occurrence("value = value + 1", "value =")
        ),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );

    let local_target = occurrence("for (value in values) {", "value");
    assert_eq!(
        variable_kinds_at(&analysis.references, local_target),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    assert_eq!(
        variable_kinds_at(&analysis.references, occurrence("value += 1", "value")),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    let local_call = occurrence("value();", "value");
    assert_eq!(
        variable_kinds_at(&analysis.references, local_call),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != local_call
    }));
}

#[test]
fn creates_no_new_binding_and_supports_single_statement_target() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/existing_for_in.dart", SOURCE));

    let value_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect();
    assert_eq!(value_bindings.len(), 2);
    assert_eq!(
        value_bindings
            .iter()
            .map(|binding| binding.kind)
            .collect::<Vec<_>>(),
        [
            DartLexicalBindingKind::Parameter,
            DartLexicalBindingKind::LocalVariable,
        ]
    );
    assert!(
        value_bindings
            .iter()
            .all(|binding| !binding.symbol_id.contains("/for_variable:value@"))
    );
    assert!(analysis.bindings.iter().any(|binding| {
        binding.name == "declared" && binding.symbol_id.contains("/for_variable:declared@")
    }));

    let declared_target = occurrence("final declared in values", "declared");
    assert!(variable_kinds_at(&analysis.references, declared_target).is_empty());

    let single_target = occurrence("for (value in values) value++", "value");
    assert_eq!(
        variable_kinds_at(&analysis.references, single_target),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    let single_body = occurrence("value++", "value");
    assert_eq!(
        variable_kinds_at(&analysis.references, single_body),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}

fn variable_references_at(
    references: &[DartIdentifierReference],
    byte_start: usize,
) -> Vec<&DartIdentifierReference> {
    references
        .iter()
        .filter(|reference| reference.span.byte_start == byte_start)
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::VariableRead
                    | DartIdentifierReferenceKind::VariableWrite
            )
        })
        .collect()
}

fn variable_kinds_at(
    references: &[DartIdentifierReference],
    byte_start: usize,
) -> Vec<DartIdentifierReferenceKind> {
    variable_references_at(references, byte_start)
        .into_iter()
        .map(|reference| reference.kind)
        .collect()
}
