from pathlib import Path

controls = Path("crates/dartscope-parse/src/lexical_regions/controls.rs")
source = controls.read_text(encoding="utf-8")
old_block = '''    if bytes.get(start) == Some(&b'{') {
        return matching_delimiter(source, start, b'{', b'}', bytes.len()).map(|end| end + 1);
    }
'''
new_block = '''    if bytes.get(start) == Some(&b'{') {
        return braced_statement_end(source, start);
    }
'''
if source.count(old_block) != 1:
    raise SystemExit("statement block boundary not found exactly once")
source = source.replace(old_block, new_block, 1)
old_match = '''        "do" => do_statement_end(source, token.end),
        _ => terminated_statement_end(source, start),
'''
new_match = '''        "do" => do_statement_end(source, token.end),
        "try" => try_statement_end(source, token.end),
        _ => terminated_statement_end(source, start),
'''
if source.count(old_match) != 1:
    raise SystemExit("statement keyword dispatch not found exactly once")
source = source.replace(old_match, new_match, 1)
marker = "\nfn terminated_statement_end(source: &str, start: usize) -> Option<usize> {\n"
if source.count(marker) != 1:
    raise SystemExit("terminated statement helper marker not found exactly once")
helpers = r'''

fn try_statement_end(source: &str, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut end = braced_statement_end(source, keyword_end)?;
    let mut saw_handler = false;
    loop {
        let Some(clause_start) = next_non_whitespace(bytes, end) else {
            return saw_handler.then_some(end);
        };
        let Some(clause) = identifier_at(source, clause_start) else {
            return saw_handler.then_some(end);
        };
        match clause.text {
            "on" => {
                end = on_clause_end(source, clause.end)?;
                saw_handler = true;
            }
            "catch" => {
                end = catch_clause_end(source, clause.end)?;
                saw_handler = true;
            }
            "finally" => return braced_statement_end(source, clause.end),
            _ => return saw_handler.then_some(end),
        }
    }
}

fn on_clause_end(source: &str, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut at = next_non_whitespace(bytes, keyword_end)?;
    while at < bytes.len() {
        if parens == 0 && brackets == 0 {
            if bytes[at] == b'{' {
                return braced_statement_end(source, at);
            }
            if matches!(bytes[at], b';' | b'}') {
                return None;
            }
            if let Some(token) = identifier_at(source, at) {
                if token.text == "catch" {
                    return catch_clause_end(source, token.end);
                }
                at = token.end;
                continue;
            }
        }
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => return None,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => return None,
            b']' => brackets -= 1,
            _ => {}
        }
        at += 1;
    }
    None
}

fn catch_clause_end(source: &str, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let open = next_non_whitespace(bytes, keyword_end)?;
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let close = matching_delimiter(source, open, b'(', b')', bytes.len())?;
    braced_statement_end(source, close + 1)
}

fn braced_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let open = next_non_whitespace(bytes, start)?;
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    matching_delimiter(source, open, b'{', b'}', bytes.len()).map(|end| end + 1)
}
'''
source = source.replace(marker, helpers + marker, 1)
controls.write_text(source, encoding="utf-8")

parser_test = Path("crates/dartscope-parse/tests/try_statement_for.rs")
parser_test.write_text(r'''use dartscope_core::{
    DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBindingKind,
};
use dartscope_parse::analyze_file_with_references;

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
fn models_try_handlers_as_complete_loop_bodies_without_leaking_bindings() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/try_loops.dart", SOURCE));
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
    let classic_on = occurrence(
        "on StateError catch (error) {\n      consume(index);",
        "index",
    );
    let classic_update = occurrence("index++;", "index");
    let classic_finally = occurrence("finally {\n      consume(index);", "index");
    let for_in_try = occurrence("try {\n      consume(value);", "value");
    let for_in_finally = occurrence("finally {\n      consume(value);", "value");
    let post_index = occurrence("consume(index);\n\n  for (final value", "index");
    let post_value = last_occurrence("consume(value)", "value");

    for offset in [classic_try, classic_on, classic_finally, for_in_try, for_in_finally] {
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
''', encoding="utf-8")

index_test = Path("crates/dartscope-index/tests/try_statement_for_resolution.rs")
index_test.write_text(r'''use dartscope_core::{
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
''', encoding="utf-8")
