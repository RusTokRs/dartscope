use dartscope_core::{
    DartFileInput, DartLexicalBindingKind, DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::resolve_project_variable_read_references;
use dartscope_parse::analyze_project_with_references;

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
fn resolves_loop_bindings_after_string_delimiters() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/string_delimiters.dart", SOURCE)],
        vec![],
    ));
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

    let resolutions = resolve_project_variable_read_references(&analysis);
    let body_resolutions: Vec<_> = resolutions
        .iter()
        .filter(|resolution| body_offsets.contains(&resolution.query.byte_offset))
        .collect();
    assert_eq!(body_resolutions.len(), body_offsets.len());
    for resolution in body_resolutions {
        assert_eq!(
            resolution.status,
            DartLexicalBindingResolutionStatus::Resolved
        );
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(
            resolution.candidates[0].kind,
            DartLexicalBindingKind::LocalVariable
        );
        assert!(
            resolution.candidates[0]
                .symbol_id
                .contains("/for_variable:")
        );
    }
    for offset in post_offsets {
        assert!(
            resolutions
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
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
