from pathlib import Path

controls = Path("crates/dartscope-parse/src/lexical_regions/controls.rs")
source = controls.read_text(encoding="utf-8")
if "next_non_whitespace," not in source:
    raise SystemExit("next_non_whitespace import not found")
source = source.replace("    identifier_at, is_binding_name, matching_delimiter, next_non_whitespace, top_level_assignment,\n", "    identifier_at, is_binding_name, matching_delimiter, top_level_assignment,\n", 1)
source = source.replace("next_non_whitespace(bytes,", "next_non_trivia(source,")
source = source.replace(
    "next_non_whitespace(source.as_bytes(), token.end)",
    "next_non_trivia(source, token.end)",
)
if "next_non_whitespace" in source:
    raise SystemExit("unconverted next_non_whitespace call remains")
marker = "\nfn statement_end(source: &str, start: usize) -> Option<usize> {\n"
if source.count(marker) != 1:
    raise SystemExit("statement_end marker not found exactly once")
helper = r'''

fn next_non_trivia(source: &str, mut at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'/') {
            at += 2;
            while bytes.get(at).is_some_and(|byte| *byte != b'\n') {
                at += 1;
            }
            continue;
        }
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'*') {
            at = block_comment_end(bytes, at)?;
            continue;
        }
        return (at < bytes.len()).then_some(at);
    }
}

fn block_comment_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'/') || bytes.get(start + 1) != Some(&b'*') {
        return None;
    }
    let mut depth = 1usize;
    let mut at = start + 2;
    while at < bytes.len() {
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'*') {
            depth += 1;
            at += 2;
            continue;
        }
        if bytes.get(at) == Some(&b'*') && bytes.get(at + 1) == Some(&b'/') {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return Some(at);
            }
            continue;
        }
        at += 1;
    }
    None
}
'''
source = source.replace(marker, helper + marker, 1)
controls.write_text(source, encoding="utf-8")

parser_test = Path("crates/dartscope-parse/tests/commented_statement_for.rs")
parser_test.write_text(r'''use dartscope_core::{
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
''', encoding="utf-8")

index_test = Path("crates/dartscope-index/tests/commented_statement_for_resolution.rs")
index_test.write_text(r'''use dartscope_core::{
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
        assert!(resolution.candidates[0].symbol_id.contains("/for_variable:"));
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
''', encoding="utf-8")
