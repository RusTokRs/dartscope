use dartscope_core::{
    Confidence, DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? input) {}
Iterable<int> choose(Iterable<int> values, int seed) => values;

void run(int seed, Iterable<int> values) {
  for (var index = seed; index < seed + 2; index++) index();
  consume(seed);
  for (final item in choose(values, seed)) item++;
  consume(seed);
  for (seed in choose(values, seed)) seed += 1;
  consume(seed);
  for (var deferred = seed; deferred < 1; deferred++)
    for (var nested = deferred; nested < 1; nested++) consume(nested);
  consume(seed);
}
"#;

#[test]
fn supports_single_statement_classic_loop_scope() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/single_statement_for.dart", SOURCE));

    let binding = analysis
        .bindings
        .iter()
        .find(|binding| {
            binding.name == "index"
                && binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.symbol_id.contains("/for_variable:index@")
        })
        .expect("classic loop binding");
    assert_eq!(
        &SOURCE[binding.declaration_span.byte_start..binding.declaration_span.byte_end],
        "index"
    );
    for offset in [
        occurrence("index < seed + 2", "index"),
        occurrence("index++) index()", "index"),
        occurrence("index();", "index"),
    ] {
        assert!(
            binding.scope_span.byte_start <= offset && offset < binding.scope_span.byte_end,
            "binding scope should contain offset {offset}"
        );
    }
    assert_eq!(binding.scope_span.byte_end, occurrence("index();", ";") + 1);

    for offset in [
        occurrence("var index = seed", "seed"),
        occurrence("index < seed + 2", "seed"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }
    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            occurrence("index < seed + 2", "index")
        ),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            occurrence("index++) index()", "index")
        ),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    let body = occurrence("index();", "index");
    assert_eq!(
        variable_kinds_at(&analysis.references, body),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != body
    }));
}

#[test]
fn supports_declared_and_existing_single_statement_for_in() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/single_statement_for.dart", SOURCE));

    let item_binding = analysis
        .bindings
        .iter()
        .find(|binding| binding.name == "item" && binding.symbol_id.contains("/for_variable:item@"))
        .expect("for-in binding");
    let item_target = occurrence("final item in choose", "item");
    assert!(variable_kinds_at(&analysis.references, item_target).is_empty());
    let item_body = occurrence("item++;", "item");
    assert!(
        item_binding.scope_span.byte_start <= item_body
            && item_body < item_binding.scope_span.byte_end
    );
    assert_eq!(
        item_binding.scope_span.byte_end,
        occurrence("item++;", ";") + 1
    );
    assert_eq!(
        variable_kinds_at(&analysis.references, item_body),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );

    for offset in [
        occurrence("choose(values, seed)) item", "values"),
        occurrence("choose(values, seed)) item", "seed"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }

    let target = occurrence("for (seed in choose", "seed");
    assert_eq!(
        variable_kinds_at(&analysis.references, target),
        vec![DartIdentifierReferenceKind::VariableWrite]
    );
    let target_reference = variable_references_at(&analysis.references, target)
        .into_iter()
        .next()
        .expect("existing-variable target");
    assert_eq!(target_reference.confidence, Confidence::High);
    assert_eq!(
        &SOURCE[target_reference.span.byte_start..target_reference.span.byte_end],
        "seed"
    );
    assert_eq!(
        variable_kinds_at(&analysis.references, occurrence("seed += 1", "seed")),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
}

#[test]
fn defers_nested_control_and_preserves_following_boundary() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/single_statement_for.dart", SOURCE));

    assert!(
        analysis
            .bindings
            .iter()
            .all(|binding| { !matches!(binding.name.as_str(), "deferred" | "nested") })
    );
    for offset in [
        occurrence("var deferred = seed", "seed"),
        occurrence("deferred < 1", "deferred"),
        occurrence("var nested = deferred", "deferred"),
        occurrence("nested < 1", "nested"),
        occurrence("consume(nested)", "nested"),
    ] {
        assert!(variable_kinds_at(&analysis.references, offset).is_empty());
    }

    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            last_occurrence("consume(seed)", "seed")
        ),
        vec![DartIdentifierReferenceKind::VariableRead]
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
