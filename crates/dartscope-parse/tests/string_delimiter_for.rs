use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? first, [Object? second]) {}
bool keepGoing(String value) => true;
String format(String value) => value;

void run(Iterable<int> values) {
  for (var headerValue = 0; keepGoing(")"); headerValue++)
    consume(";", headerValue);
  consume(headerValue);

  for (final rawValue in values) {
    consume(r'''}''');
    consume(rawValue);
  }
  consume(rawValue);

  for (final interpolatedValue in values) {
    consume("${format(')')}");
    consume(interpolatedValue);
  }
  consume(interpolatedValue);
}
"#;

#[test]
fn ignores_string_delimiters_when_bounding_loop_statements() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/string_delimiters.dart", SOURCE));
    let loop_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| {
            binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.symbol_id.contains("/for_variable:")
        })
        .collect();
    assert_eq!(loop_bindings.len(), 3);

    let body_offsets = [
        occurrence("consume(\";\", headerValue);", "headerValue"),
        occurrence("consume(rawValue);", "rawValue"),
        occurrence("consume(interpolatedValue);", "interpolatedValue"),
    ];
    let post_offsets = [
        occurrence("consume(headerValue);\n\n  for", "headerValue"),
        occurrence("consume(rawValue);\n\n  for", "rawValue"),
        last_occurrence("consume(interpolatedValue);", "interpolatedValue"),
    ];

    for offset in body_offsets {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }
    for offset in post_offsets {
        assert!(variable_kinds_at(&analysis.references, offset).is_empty());
    }
    for binding in loop_bindings {
        let post = match binding.name.as_str() {
            "headerValue" => post_offsets[0],
            "rawValue" => post_offsets[1],
            "interpolatedValue" => post_offsets[2],
            other => panic!("unexpected loop binding {other}"),
        };
        assert!(binding.scope_span.byte_end <= post);
    }
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

fn variable_kinds_at(
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
