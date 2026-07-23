use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? value) {}

void run(bool enabled, Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    /* before body */
    if (enabled)
      consume(index);
    /* between branches */
    else
      index++;
  consume(index);

  for (final value in values)
    // before try
    try {
      consume(value);
    }
    /* before handler */
    on StateError {
      consume(value);
    }
    // before finally
    finally {
      consume(value);
    }
  consume(value);
}
"#;

#[test]
fn skips_comments_when_bounding_unbraced_loop_statements() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/commented_loops.dart", SOURCE));
    let loop_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| {
            binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.symbol_id.contains("/for_variable:")
        })
        .collect();
    assert_eq!(loop_bindings.len(), 2);

    let classic_read = occurrence("consume(index);", "index");
    let classic_update = occurrence("index++;", "index");
    let for_in_try = occurrence("try {\n      consume(value);", "value");
    let for_in_handler = occurrence("on StateError {\n      consume(value);", "value");
    let for_in_finally = occurrence("finally {\n      consume(value);", "value");
    let post_index = occurrence("consume(index);\n\n  for", "index");
    let post_value = last_occurrence("consume(value);", "value");

    for offset in [classic_read, for_in_try, for_in_handler, for_in_finally] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }
    assert_eq!(
        variable_kinds_at(&analysis.references, classic_update),
        vec![
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    for offset in [post_index, post_value] {
        assert!(variable_kinds_at(&analysis.references, offset).is_empty());
    }
    for binding in loop_bindings {
        let post = if binding.name == "index" {
            post_index
        } else {
            post_value
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
