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
Object? topLevel() => null;

void run(
  int seed,
  int self,
  int laterName,
) {
  for (
    var first = seed, second = first, third;
    first < second + seed;
    first++, second += first
  ) {
    consume(first);
    consume(second);
    third = second;
  }
  for (
    var callback = topLevel, result = callback();
    result != null;
    callback = topLevel
  ) consume(result);
  for (var self = self, copy = self; copy != null; ) consume(copy);
  for (var before = laterName, laterName = seed; before < seed; before++) consume(before);
  for (var early = pendingCall(), pendingCall = topLevel; early != null; ) consume(early);
  for (seed = 0, laterName = seed; seed < 1; seed++) consume(seed);
  for (var (left, right) = values; seed < 1; seed++) consume(seed);
  consume(seed);
}
"#;

#[test]
fn resolves_multi_declarator_intervals_without_reparsing() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    assert_resolution(
        &reads,
        occurrence("var first = seed", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
    for (offset, name) in [
        (occurrence("second = first", "first"), "first"),
        (occurrence("first < second + seed", "first"), "first"),
        (occurrence("first < second + seed", "second"), "second"),
        (occurrence("consume(first)", "first"), "first"),
        (occurrence("consume(second)", "second"), "second"),
        (occurrence("third = second", "second"), "second"),
        (occurrence("result = callback()", "callback"), "callback"),
        (occurrence("consume(result)", "result"), "result"),
        (occurrence("copy = self", "self"), "self"),
    ] {
        assert_resolution(
            &reads,
            offset,
            DartLexicalBindingKind::LocalVariable,
            &format!("/for_variable:{name}@"),
        );
    }
    assert_same_resolution(
        &reads,
        &writes,
        occurrence("first++, second", "first"),
        "/for_variable:first@",
    );
    assert_same_resolution(
        &reads,
        &writes,
        occurrence("second += first", "second"),
        "/for_variable:second@",
    );
    assert_resolution(
        &writes,
        occurrence("third = second", "third"),
        DartLexicalBindingKind::LocalVariable,
        "/for_variable:third@",
    );
    assert_resolution(
        &writes,
        last_occurrence("callback = topLevel", "callback"),
        DartLexicalBindingKind::LocalVariable,
        "/for_variable:callback@",
    );
}

#[test]
fn keeps_self_later_and_unsupported_headers_out_of_resolution() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    for offset in [
        occurrence("var self = self", "self = self") + "self = ".len(),
        occurrence("before = laterName", "laterName"),
        occurrence("early = pendingCall()", "pendingCall"),
        occurrence("seed = 0, laterName", "seed"),
        occurrence("laterName = seed; seed < 1", "laterName"),
        occurrence("var (left, right)", "left"),
        occurrence("var (left, right)", "right"),
    ] {
        assert!(reads.iter().all(|resolution| resolution.query.byte_offset != offset));
        assert!(writes
            .iter()
            .all(|resolution| resolution.query.byte_offset != offset));
    }

    let pending_call = occurrence("early = pendingCall()", "pendingCall");
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != pending_call
    }));

    let callback = occurrence("result = callback()", "callback");
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != callback
    }));
    let namespace = resolve_project_identifier_references(&analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        !matches!(
            resolution.reference.kind,
            DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
        )
    }));
    assert_resolution(
        &reads,
        last_occurrence("consume(seed)", "seed"),
        DartLexicalBindingKind::Parameter,
        "/parameter:seed",
    );
}

fn assert_same_resolution(
    reads: &[DartLexicalBindingResolution],
    writes: &[DartLexicalBindingResolution],
    byte_offset: usize,
    symbol_fragment: &str,
) {
    let read = resolution_at(reads, byte_offset);
    let write = resolution_at(writes, byte_offset);
    assert_eq!(read.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(write.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(read.candidates.len(), 1);
    assert_eq!(write.candidates.len(), 1);
    assert_eq!(read.candidates[0].symbol_id, write.candidates[0].symbol_id);
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
    assert!(
        resolution.candidates[0].symbol_id.contains(symbol_fragment),
        "resolution at {byte_offset}: expected {symbol_fragment}, got {}",
        resolution.candidates[0].symbol_id
    );
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
