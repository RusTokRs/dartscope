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
fn filters_nested_write_targets_from_deferred_loop_regions() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/deferred_loop.dart", SOURCE));

    for offset in [
        occurrence("var outer = 0", "outer"),
        occurrence("outer < 1", "outer"),
        occurrence("outer++)", "outer"),
        occurrence("for (value in values)", "value"),
        occurrence("value++;", "value"),
    ] {
        assert!(analysis.references.iter().all(|reference| {
            reference.span.byte_start != offset
                || !matches!(
                    reference.kind,
                    DartIdentifierReferenceKind::VariableRead
                        | DartIdentifierReferenceKind::VariableWrite
                )
        }));
    }

    let following = occurrence("consume(value)", "value");
    assert!(analysis.references.iter().any(|reference| {
        reference.span.byte_start == following
            && reference.kind == DartIdentifierReferenceKind::VariableRead
    }));
}

fn occurrence(fragment: &str, token: &str) -> usize {
    let start = SOURCE.find(fragment).expect("fragment");
    start
        + SOURCE[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
