use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind,
    DartLexicalBindingResolution, DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_identifier_references, resolve_project_variable_read_references,
    resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

const SOURCE: &str = r#"
void consume(Object? input) {}
Iterable<int> choose(Iterable<int> values, int seed) => values;

void run(int seed, Iterable<int> values) {
  for (var index = seed; index < seed + 2; index++) index();
  consume(seed);
  for (final item in choose(values, seed)) item++;
  consume(seed);
  for (seed in choose(values, seed)) seed += 1;
  consume(seed);
  for (var deferred = seed; deferred < 1; deferred++)
    for (var nested = deferred; nested < 1; nested++) consume(nested);
  consume(seed);
}
"#;

#[test]
fn resolves_single_statement_headers_and_bodies() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    assert_resolution(
        &reads,
        occurrence("var index = seed", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
    for offset in [
        occurrence("index < seed + 2", "index"),
        occurrence("index();", "index"),
    ] {
        assert_resolution(
            &reads,
            offset,
            DartLexicalBindingKind::LocalVariable,
            "/for_variable:index@",
        );
    }
    assert_same_resolution(
        &reads,
        &writes,
        occurrence("index++) index()", "index"),
        DartLexicalBindingKind::LocalVariable,
        "/for_variable:index@",
    );

    assert_same_resolution(
        &reads,
        &writes,
        occurrence("item++;", "item"),
        DartLexicalBindingKind::LocalVariable,
        "/for_variable:item@",
    );
    assert_resolution(
        &writes,
        occurrence("for (seed in choose", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
    assert_same_resolution(
        &reads,
        &writes,
        occurrence("seed += 1", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
    assert_resolution(
        &reads,
        last_occurrence("consume(seed)", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
}

#[test]
fn keeps_nested_control_deferred_and_namespace_filtered() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    for offset in [
        occurrence("var deferred = seed", "seed"),
        occurrence("deferred < 1", "deferred"),
        occurrence("var nested = deferred", "deferred"),
        occurrence("nested < 1", "nested"),
        occurrence("consume(nested)", "nested"),
    ] {
        assert!(reads.iter().all(|resolution| resolution.query.byte_offset != offset));
        assert!(writes
            .iter()
            .all(|resolution| resolution.query.byte_offset != offset));
    }

    let body_call = occurrence("index();", "index");
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != body_call
    }));
    let namespace = resolve_project_identifier_references(&analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        !matches!(
            resolution.reference.kind,
            DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
        )
    }));
}

fn assert_same_resolution(
    reads: &[DartLexicalBindingResolution],
    writes: &[DartLexicalBindingResolution],
    byte_offset: usize,
    kind: DartLexicalBindingKind,
    symbol_fragment: &str,
) {
    let read = resolution_at(reads, byte_offset);
    let write = resolution_at(writes, byte_offset);
    assert_eq!(read.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(write.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(read.candidates.len(), 1);
    assert_eq!(write.candidates.len(), 1);
    assert_eq!(read.candidates[0].symbol_id, write.candidates[0].symbol_id);
    assert_eq!(read.candidates[0].kind, kind);
    assert!(read.candidates[0].symbol_id.contains(symbol_fragment));
}

fn assert_resolution(
    resolutions: &[DartLexicalBindingResolution],
    byte_offset: usize,
    kind: DartLexicalBindingKind,
    symbol_fragment: &str,
) {
    let resolution = resolution_at(resolutions, byte_offset);
    assert_eq!(
        resolution.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(resolution.candidates.len(), 1);
    assert_eq!(resolution.candidates[0].kind, kind);
    assert!(resolution.candidates[0].symbol_id.contains(symbol_fragment));
}

fn resolution_at(
    resolutions: &[DartLexicalBindingResolution],
    byte_offset: usize,
) -> &DartLexicalBindingResolution {
    resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == byte_offset)
        .unwrap_or_else(|| panic!("missing resolution at {byte_offset}"))
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
