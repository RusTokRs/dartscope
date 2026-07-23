use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
void consume(Object? value) {}

void run(Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    try {
      consume(index);
    } on StateError {
      consume(index);
    } on FormatException catch (error) {
      consume(index);
      consume(error);
    } catch (error, stack) {
      index++;
      consume(error);
      consume(stack);
    } finally {
      consume(index);
    }
  consume(index);

  for (final value in values)
    try {
      consume(value);
    } finally {
      consume(value);
    }
  consume(value);
}
"#;

#[test]
fn models_try_handlers_as_complete_loop_bodies_without_leaking_bindings() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/try_loops.dart", SOURCE));
    let loop_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| {
            binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.symbol_id.contains("/for_variable:")
        })
        .collect();
    let mut names: Vec<_> = loop_bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["index", "value"]);

    let classic_try = occurrence("try {\n      consume(index);", "index");
    let classic_on = occurrence("on StateError {\n      consume(index);", "index");
    let classic_on_catch = occurrence(
        "on FormatException catch (error) {\n      consume(index);",
        "index",
    );
    let classic_update = occurrence("index++;", "index");
    let classic_finally = occurrence("finally {\n      consume(index);", "index");
    let for_in_try = occurrence("try {\n      consume(value);", "value");
    let for_in_finally = occurrence("finally {\n      consume(value);", "value");
    let post_index = occurrence("consume(index);\n\n  for (final value", "index");
    let post_value = last_occurrence("consume(value)", "value");

    for offset in [
        classic_try,
        classic_on,
        classic_on_catch,
        classic_finally,
        for_in_try,
        for_in_finally,
    ] {
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
