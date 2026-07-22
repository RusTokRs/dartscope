use dartscope_core::{
    DartFileInput, DartLexicalBindingKind, DartLexicalBindingResolution,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_variable_read_references, resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

const SOURCE: &str = r#"
void consume(Object? value) {}

void run(Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    try {
      consume(index);
    } on StateError catch (error) {
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
fn resolves_loop_references_inside_try_handlers_without_post_loop_leakage() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/try_loops.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    for offset in [
        occurrence("try {\n      consume(index);", "index"),
        occurrence(
            "on StateError catch (error) {\n      consume(index);",
            "index",
        ),
        occurrence("finally {\n      consume(index);", "index"),
    ] {
        assert_loop_resolution(&reads, offset, "/for_variable:index@");
    }
    let update = occurrence("index++;", "index");
    assert_loop_resolution(&reads, update, "/for_variable:index@");
    assert_loop_resolution(&writes, update, "/for_variable:index@");

    for offset in [
        occurrence("try {\n      consume(value);", "value"),
        occurrence("finally {\n      consume(value);", "value"),
    ] {
        assert_loop_resolution(&reads, offset, "/for_variable:value@");
    }

    for offset in [
        occurrence("consume(index);\n\n  for (final value", "index"),
        last_occurrence("consume(value)", "value"),
    ] {
        assert!(
            reads
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
        assert!(
            writes
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
    }
}

fn assert_loop_resolution(
    resolutions: &[DartLexicalBindingResolution],
    byte_offset: usize,
    symbol_fragment: &str,
) {
    let resolution = resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == byte_offset)
        .expect("loop resolution");
    assert_eq!(
        resolution.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(resolution.candidates.len(), 1);
    assert_eq!(
        resolution.candidates[0].kind,
        DartLexicalBindingKind::LocalVariable
    );
    assert!(resolution.candidates[0].symbol_id.contains(symbol_fragment));
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
