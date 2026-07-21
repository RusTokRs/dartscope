use dartscope_core::{
    Confidence, DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBinding, DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? input) {}
Object? topLevel() => null;

void run(
  int seed,
  int self,
  int laterName,
  Object? Function() future,
) {
  for (
    var first = seed, second = first, third;
    first < second + seed;
    first++, second += first
  ) {
    consume(first);
    consume(second);
    third = second;
  }
  consume(seed);
  for (
    var callback = topLevel, result = callback();
    result != null;
    callback = topLevel
  ) consume(result);
  for (var self = self, copy = self; copy != null; ) consume(copy);
  for (var before = laterName, laterName = seed; before < seed; before++) consume(before);
  for (var early = future(), future = topLevel; early != null; ) consume(early);
  for (seed = 0, laterName = seed; seed < 1; seed++) consume(seed);
  for (var (left, right) = values; seed < 1; seed++) consume(seed);
  consume(seed);
}
"#;

#[test]
fn models_per_declarator_intervals_and_stable_ids() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/multi_declarator_for.dart", SOURCE));

    assert_binding(
        &analysis.bindings,
        "first",
        occurrence("var first = seed", "first"),
        end_of("var first = seed"),
    );
    assert_binding(
        &analysis.bindings,
        "second",
        occurrence("second = first", "second"),
        end_of("second = first"),
    );
    assert_binding(
        &analysis.bindings,
        "third",
        occurrence("second = first, third", "third"),
        occurrence("second = first, third", "third") + "third".len(),
    );
    assert_binding(
        &analysis.bindings,
        "callback",
        occurrence("var callback = topLevel", "callback"),
        end_of("var callback = topLevel"),
    );
    assert_binding(
        &analysis.bindings,
        "result",
        occurrence("result = callback()", "result"),
        end_of("result = callback()"),
    );
}

#[test]
fn emits_ordered_initializer_condition_update_and_body_accesses() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/multi_declarator_for.dart", SOURCE));

    for offset in [
        occurrence("var first = seed", "seed"),
        occurrence("second = first", "first"),
        occurrence("first < second + seed", "first"),
        occurrence("first < second + seed", "second"),
        occurrence("first < second + seed", "seed"),
        occurrence("consume(first)", "first"),
        occurrence("consume(second)", "second"),
        occurrence("third = second", "second"),
        occurrence("result = callback()", "callback"),
        occurrence("result != null", "result"),
        occurrence("consume(result)", "result"),
        occurrence("copy = self", "self"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead],
            "unexpected reference kinds at {offset}"
        );
    }
    for offset in [
        occurrence("first++, second", "first"),
        occurrence("second += first", "second"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![
                DartIdentifierReferenceKind::VariableRead,
                DartIdentifierReferenceKind::VariableWrite,
            ]
        );
    }
    assert_eq!(
        variable_kinds_at(&analysis.references, occurrence("third = second", "third")),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            last_occurrence("callback = topLevel", "callback")
        ),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );

    let callback = occurrence("result = callback()", "callback");
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != callback
    }));
    assert!(
        variable_references_at(&analysis.references, callback)
            .iter()
            .all(|reference| reference.confidence == Confidence::High)
    );
}

#[test]
fn suppresses_self_later_and_unsupported_multi_initializers() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/multi_declarator_for.dart", SOURCE));

    for offset in [
        occurrence("var self = self", "self = self") + "self = ".len(),
        occurrence("before = laterName", "laterName"),
        occurrence("early = future()", "future"),
        occurrence("seed = 0, laterName", "seed"),
        occurrence("laterName = seed; seed < 1", "laterName"),
        occurrence("var (left, right)", "left"),
        occurrence("var (left, right)", "right"),
    ] {
        assert!(
            variable_kinds_at(&analysis.references, offset).is_empty(),
            "unexpected lexical reference at {offset}"
        );
    }
    assert!(analysis.bindings.iter().all(|binding| {
        !matches!(binding.name.as_str(), "left" | "right")
    }));
    assert_eq!(
        variable_kinds_at(&analysis.references, last_occurrence("consume(seed)", "seed")),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
}

fn assert_binding(
    bindings: &[DartLexicalBinding],
    name: &str,
    declaration_start: usize,
    scope_start: usize,
) {
    let binding = bindings
        .iter()
        .find(|binding| {
            binding.name == name
                && binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.declaration_span.byte_start == declaration_start
        })
        .unwrap_or_else(|| panic!("missing binding {name} at {declaration_start}"));
    assert_eq!(binding.declaration_span.byte_end, declaration_start + name.len());
    assert_eq!(binding.scope_span.byte_start, scope_start);
    assert!(
        binding
            .symbol_id
            .ends_with(&format!("/for_variable:{name}@{declaration_start}"))
    );
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}

fn last_occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.rfind(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}

fn end_of(fragment: &str) -> usize {
    SOURCE.find(fragment).expect("fragment") + fragment.len()
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
