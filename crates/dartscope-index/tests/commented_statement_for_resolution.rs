use dartscope_core::{
    DartFileInput, DartLexicalBindingKind, DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_variable_read_references, resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

const SOURCE: &str = r#"
void consume(Object? value) {}

void run(bool enabled, Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    /* before body */
    if (enabled)
      consume(index);
    // between branches
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
fn resolves_loop_bindings_across_commented_statement_boundaries() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/commented_loops.dart", SOURCE)],
        vec![],
    ));
    let classic_read = occurrence("consume(index);", "index");
    let classic_update = occurrence("index++;", "index");
    let for_in_try = occurrence("try {\n      consume(value);", "value");
    let for_in_handler = occurrence("on StateError {\n      consume(value);", "value");
    let for_in_finally = occurrence("finally {\n      consume(value);", "value");
    let post_index = occurrence("consume(index);\n\n  for", "index");
    let post_value = last_occurrence("consume(value);", "value");
    let read_offsets = [
        classic_read,
        classic_update,
        for_in_try,
        for_in_handler,
        for_in_finally,
    ];

    let reads: Vec<_> = resolve_project_variable_read_references(&analysis)
        .into_iter()
        .filter(|resolution| read_offsets.contains(&resolution.query.byte_offset))
        .collect();
    let writes: Vec<_> = resolve_project_variable_write_references(&analysis)
        .into_iter()
        .filter(|resolution| resolution.query.byte_offset == classic_update)
        .collect();

    assert_eq!(reads.len(), read_offsets.len());
    assert_eq!(writes.len(), 1);
    for resolution in reads.iter().chain(&writes) {
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
    for offset in [post_index, post_value] {
        assert!(
            resolve_project_variable_read_references(&analysis)
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
