from pathlib import Path

path = Path("crates/dartscope-parse/tests/deferred_region_writes.rs")
source = path.read_text(encoding="utf-8")
start_marker = "#[test]\nfn filters_nested_write_targets_from_deferred_loop_regions()"
start = source.index(start_marker)
end = source.index("\nfn occurrence(", start)
replacement = r'''#[test]
fn models_nested_loop_write_targets_without_leaking_bindings() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/deferred_loop.dart", SOURCE));

    assert!(
        kinds_at(&analysis.references, occurrence("var outer = 0", "outer")).is_empty()
    );
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
        kinds_at(
            &analysis.references,
            occurrence("consume(value)", "value")
        ),
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
'''
source = source[:start] + replacement + source[end:]
path.write_text(source, encoding="utf-8")
