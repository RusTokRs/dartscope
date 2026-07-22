use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_identifier_references, resolve_project_variable_read_references,
    resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

const SOURCE: &str = r#"
void run(int value, Iterable<int> values) {
  values.forEach((value) {
    value++;
    value();
  });
  for (final value in values) {
    value += 1;
    value();
  }
  for (var index = 0; index < 2; index++) {
    index--;
    index();
  }
  try {
    consume(value);
  } catch (value, stack) {
    value++;
    value();
    stack();
    consume(stack);
  }
  value--;
}
"#;

#[test]
fn resolves_region_accesses_to_the_same_most_specific_binding() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", SOURCE)],
        vec![],
    ));
    let offsets = [
        SOURCE.find("value++").expect("closure update"),
        SOURCE.find("value += 1").expect("for-in update"),
        SOURCE.find("index--").expect("classic loop update"),
        SOURCE.rfind("value++").expect("catch update"),
        SOURCE.rfind("value--").expect("outer update"),
    ];

    let reads: Vec<_> = resolve_project_variable_read_references(&analysis)
        .into_iter()
        .filter(|resolution| offsets.contains(&resolution.query.byte_offset))
        .collect();
    let writes: Vec<_> = resolve_project_variable_write_references(&analysis)
        .into_iter()
        .filter(|resolution| offsets.contains(&resolution.query.byte_offset))
        .collect();
    assert_eq!(reads.len(), offsets.len());
    assert_eq!(writes.len(), offsets.len());
    for resolution in reads.iter().chain(&writes) {
        assert_eq!(
            resolution.status,
            DartLexicalBindingResolutionStatus::Resolved
        );
        assert_eq!(resolution.candidates.len(), 1);
    }
    assert!(reads.iter().zip(&writes).all(|(read, write)| {
        read.query.byte_offset == write.query.byte_offset
            && read.candidates[0].symbol_id == write.candidates[0].symbol_id
    }));
    assert_expected_bindings(&reads);
    assert_namespace_filters_lexical_accesses(&analysis);
}

#[test]
fn resolves_loop_accesses_inside_nested_unbraced_if_else_statements() {
    let source = r#"
void consume(int value) {}

void run(bool enabled, Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    if (enabled)
      consume(index);
    else
      index++;
  for (final value in values)
    if (enabled)
      consume(value);
    else
      value++;
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/nested_loops.dart", source)],
        vec![],
    ));
    let classic_read = source.find("consume(index)").expect("classic body read") + "consume(".len();
    let classic_update = source.rfind("index++").expect("classic else update");
    let for_in_read = source.find("consume(value)").expect("for-in body read") + "consume(".len();
    let for_in_update = source.find("value++").expect("for-in else update");
    let read_offsets = [classic_read, classic_update, for_in_read, for_in_update];
    let write_offsets = [classic_update, for_in_update];

    let reads: Vec<_> = resolve_project_variable_read_references(&analysis)
        .into_iter()
        .filter(|resolution| read_offsets.contains(&resolution.query.byte_offset))
        .collect();
    let writes: Vec<_> = resolve_project_variable_write_references(&analysis)
        .into_iter()
        .filter(|resolution| write_offsets.contains(&resolution.query.byte_offset))
        .collect();

    assert_eq!(reads.len(), read_offsets.len());
    assert_eq!(writes.len(), write_offsets.len());
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
}

fn assert_expected_bindings(resolutions: &[dartscope_core::DartLexicalBindingResolution]) {
    assert_eq!(
        resolutions
            .iter()
            .map(|resolution| resolution.candidates[0].kind)
            .collect::<Vec<_>>(),
        [
            DartLexicalBindingKind::Parameter,
            DartLexicalBindingKind::LocalVariable,
            DartLexicalBindingKind::LocalVariable,
            DartLexicalBindingKind::LocalVariable,
            DartLexicalBindingKind::Parameter,
        ]
    );
    let symbol_ids: Vec<_> = resolutions
        .iter()
        .map(|resolution| resolution.candidates[0].symbol_id.as_str())
        .collect();
    assert!(symbol_ids[0].contains("/closure_parameter:value@"));
    assert!(symbol_ids[1].contains("/for_variable:value@"));
    assert!(symbol_ids[2].contains("/for_variable:index@"));
    assert!(symbol_ids[3].contains("/catch_parameter:value@"));
    assert!(symbol_ids[4].ends_with("/parameter:value"));
}

fn assert_namespace_filters_lexical_accesses(
    analysis: &dartscope_core::DartProjectReferenceAnalysis,
) {
    let invocation_roots: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::InvocationTarget)
        .filter(|reference| matches!(reference.name.as_str(), "value" | "index" | "stack"))
        .collect();
    assert!(invocation_roots.is_empty());

    let namespace = resolve_project_identifier_references(analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        !matches!(
            resolution.reference.kind,
            DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
        )
    }));
}
