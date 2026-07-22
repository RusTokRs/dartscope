use dartscope_core::{DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? input) {}

void run(int value, Iterable<int> values) {
  for (var outer = 0; outer < 1; outer++)
    for (value in values) value++;
  consume(value);
}
"#;

#[test]
fn models_nested_loop_write_targets_without_leaking_bindings() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/deferred_loop.dart", SOURCE));

    assert!(kinds_at(&analysis.references, occurrence("var outer = 0", "outer")).is_empty());
    assert_eq!(
        kinds_at(&analysis.references, occurrence("outer < 1", "outer")),
        [DartIdentifierReferenceKind::VariableRead]
    );
    assert_eq!(
        kinds_at(&analysis.references, occurrence("outer++)", "outer")),
        [
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    assert_eq!(
        kinds_at(
            &analysis.references,
            occurrence("for (value in values)", "value")
        ),
        [DartIdentifierReferenceKind::VariableWrite]
    );
    assert_eq!(
        kinds_at(&analysis.references, occurrence("value++;", "value")),
        [
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    assert_eq!(
        kinds_at(&analysis.references, occurrence("consume(value)", "value")),
        [DartIdentifierReferenceKind::VariableRead]
    );
}

fn kinds_at(
    references: &[dartscope_core::DartIdentifierReference],
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

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
