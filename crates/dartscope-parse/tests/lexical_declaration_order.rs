use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBinding,
    DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? value) {}
Object? seedCallback() => null;

void run(
  int value,
  int seed,
  int self,
  int later,
  Object? Function() recursive,
  Object? Function() future,
) {
  consume(value);
  var first = seed, second = first;
  var counter = 0, updated = (counter += seed);
  var assigned = 0, assignedResult = (assigned = seed);
  int? pending, copied = pending;
  var callback = seedCallback, result = callback();
  var self = self;
  var before = later, later = seed;
  var recursive = recursive();
  var earlyCall = future(), future = seedCallback;
  var value = seed, shadowCopy = value;
  consume(value);
}
"#;

#[test]
fn models_normative_per_declarator_scope_starts() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/declaration_order.dart", SOURCE));

    assert_eq!(
        local_binding(&analysis.bindings, "first")
            .scope_span
            .byte_start,
        end_of("var first = seed")
    );
    assert_eq!(
        local_binding(&analysis.bindings, "second")
            .scope_span
            .byte_start,
        end_of("second = first")
    );
    assert_eq!(
        local_binding(&analysis.bindings, "pending")
            .scope_span
            .byte_start,
        end_of("pending")
    );
    assert_eq!(
        local_binding(&analysis.bindings, "callback")
            .scope_span
            .byte_start,
        end_of("var callback = seedCallback")
    );
    assert_eq!(
        local_binding(&analysis.bindings, "value")
            .scope_span
            .byte_start,
        end_of("var value = seed")
    );
}

#[test]
fn emits_legal_same_statement_accesses_and_suppresses_early_ones() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/declaration_order.dart", SOURCE));

    assert_eq!(
        kinds_at(&analysis.references, occurrence("second = first", "first")),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert_eq!(
        kinds_at(
            &analysis.references,
            occurrence("counter += seed", "counter")
        ),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    assert_eq!(
        kinds_at(
            &analysis.references,
            occurrence("assigned = seed", "assigned")
        ),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    assert_eq!(
        kinds_at(
            &analysis.references,
            occurrence("copied = pending", "pending")
        ),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    let callback = occurrence("result = callback()", "callback");
    assert_eq!(
        kinds_at(&analysis.references, callback),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert!(analysis.references.iter().all(|reference| reference.kind
        != DartIdentifierReferenceKind::InvocationTarget
        || reference.span.byte_start != callback));

    for offset in [
        occurrence("var self = self", "self = self") + "self = ".len(),
        occurrence("before = later", "later"),
        occurrence("recursive = recursive()", "recursive()"),
        occurrence("earlyCall = future()", "future"),
    ] {
        assert!(kinds_at(&analysis.references, offset).is_empty());
    }
}

#[test]
fn preserves_non_retroactive_shadowing_across_separate_statements() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/declaration_order.dart", SOURCE));
    let uses: Vec<_> = SOURCE
        .match_indices("consume(value)")
        .map(|(offset, _)| offset + "consume(".len())
        .collect();

    assert_eq!(uses.len(), 2);
    assert_eq!(
        kinds_at(&analysis.references, uses[0]),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert_eq!(
        kinds_at(&analysis.references, uses[1]),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    let local_value = local_binding(&analysis.bindings, "value");
    assert!(uses[0] < local_value.scope_span.byte_start);
    assert!(local_value.scope_span.byte_start <= uses[1]);
}

fn local_binding<'a>(bindings: &'a [DartLexicalBinding], name: &str) -> &'a DartLexicalBinding {
    bindings
        .iter()
        .find(|binding| {
            binding.kind == DartLexicalBindingKind::LocalVariable && binding.name == name
        })
        .unwrap_or_else(|| panic!("missing local binding {name}"))
}

fn end_of(fragment: &str) -> usize {
    SOURCE.find(fragment).expect("fragment") + fragment.len()
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}

fn kinds_at(
    references: &[DartIdentifierReference],
    byte_start: usize,
) -> Vec<DartIdentifierReferenceKind> {
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
        .map(|reference| reference.kind)
        .collect()
}
