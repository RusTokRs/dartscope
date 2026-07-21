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
Iterable<int> choose(Iterable<int> values, Object? current) => values;

void run(int value, Iterable<int> values) {
  for (value in choose(values, value)) {
    consume(value);
    value = value + 1;
  }
  {
    var value = 0;
    for (value in values) {
      value += 1;
      value();
    }
  }
  consume(value);
  for (final declared in values) {
    consume(declared);
  }
  for (value in values) value++;
}
"#;

#[test]
fn resolves_targets_iterables_and_bodies_to_existing_bindings() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    let parameter_target = occurrence("for (value in choose", "value");
    assert_resolution(
        &writes,
        parameter_target,
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );
    for offset in [
        occurrence("choose(values, value)", ", value") + 2,
        occurrence("consume(value);\n    value = value + 1", "value"),
        occurrence("value = value + 1", "value + 1"),
    ] {
        assert_resolution(
            &reads,
            offset,
            DartLexicalBindingKind::Parameter,
            "/parameter:value",
        );
    }
    assert_resolution(
        &reads,
        occurrence("choose(values, value)", "values"),
        DartLexicalBindingKind::Parameter,
        "/parameter:values",
    );
    assert_resolution(
        &writes,
        occurrence("value = value + 1", "value ="),
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );

    let local_target = occurrence("for (value in values) {", "value");
    assert_resolution(
        &writes,
        local_target,
        DartLexicalBindingKind::LocalVariable,
        "/local_variable:value",
    );
    let update = occurrence("value += 1", "value");
    let update_read = resolution_at(&reads, update);
    let update_write = resolution_at(&writes, update);
    assert_eq!(
        update_read.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(
        update_write.status,
        DartLexicalBindingResolutionStatus::Resolved
    );
    assert_eq!(
        update_read.candidates[0].symbol_id,
        update_write.candidates[0].symbol_id
    );
    assert!(
        update_read.candidates[0]
            .symbol_id
            .ends_with("/local_variable:value")
    );

    assert_resolution(
        &reads,
        occurrence("consume(value);\n  for (final declared", "value"),
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );
}

#[test]
fn keeps_declared_target_omitted_and_resolves_single_statement() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let reads = resolve_project_variable_read_references(&analysis);
    let writes = resolve_project_variable_write_references(&analysis);

    let declared = occurrence("final declared in values", "declared");
    assert!(
        writes
            .iter()
            .all(|resolution| resolution.query.byte_offset != declared)
    );

    let single_target = occurrence("for (value in values) value++", "value");
    assert_resolution(
        &writes,
        single_target,
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );
    let single_body = occurrence("value++", "value");
    assert_resolution(
        &reads,
        single_body,
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );
    assert_resolution(
        &writes,
        single_body,
        DartLexicalBindingKind::Parameter,
        "/parameter:value",
    );

    let local_call = occurrence("value();", "value");
    assert!(analysis.references.iter().all(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || reference.span.byte_start != local_call
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
    assert!(
        resolution.candidates[0].symbol_id.ends_with(symbol_suffix),
        "resolution at {byte_offset}: expected suffix {symbol_suffix}, got {}",
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
