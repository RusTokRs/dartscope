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
void use(Object? value) {}
Object? topLevel() => null;

void run(int value, int seed) {
  use(value);
  var first = seed, second = first;
  var counter = 0, updated = (counter += seed);
  var assigned = 0, assignedResult = (assigned = seed);
  int? pending, copied = pending;
  var callback = topLevel, result = callback();
  var value = seed, copiedValue = value;
  use(value);
}
"#;

#[test]
fn resolves_same_statement_accesses_to_earlier_declarators() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    assert_resolution(
        &reads,
        occurrence("second = first", "first"),
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:first",
    );
    let counter = occurrence("counter += seed", "counter");
    let counter_read = resolution_at(&reads, counter);
    let counter_write = resolution_at(&writes, counter);
    assert_eq!(
        counter_read.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(
        counter_write.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(
        counter_read.candidates[0].symbol_id,
        counter_write.candidates[0].symbol_id
    );
    assert_resolution(
        &writes,
        occurrence("assigned = seed", "assigned"),
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:assigned",
    );
    assert_resolution(
        &reads,
        occurrence("copied = pending", "pending"),
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:pending",
    );
    assert_resolution(
        &reads,
        occurrence("result = callback()", "callback"),
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:callback",
    );
    assert_resolution(
        &reads,
        occurrence("copiedValue = value", "value"),
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:value",
    );
}

#[test]
fn keeps_separate_statement_compatibility_and_namespace_filtering() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let uses: Vec<_> = SOURCE
        .match_indices("use(value)")
        .map(|(offset, _)| offset + "use(".len())
        .collect();

    assert_eq!(uses.len(), 2);
    assert_resolution(
        &reads,
        uses[0],
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );
    assert_resolution(
        &reads,
        uses[1],
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:value",
    );

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
}

fn assert_resolution(
    resolutions: &[DartLexicalBindingResolution],
    byte_offset: usize,
    kind: DartLexicalBindingKind,
    symbol_suffix: &str,
) {
    let resolution = resolution_at(resolutions, byte_offset);
    assert_eq!(
        resolution.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(resolution.candidates.len(), 1);
    assert_eq!(resolution.candidates[0].kind, kind);
    assert!(resolution.candidates[0].symbol_id.ends_with(symbol_suffix));
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
