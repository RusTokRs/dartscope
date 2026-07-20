use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_identifier_references, resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_simple_assignment_targets_through_lexical_binding_intervals() {
    let source = r#"
void run(int value, int other) {
  value = other;
  {
    var value = 0;
    value = other;
  }
  value = other;
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));
    let writes: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableWrite)
        .collect();
    assert_eq!(writes.len(), 3);

    let lexical = resolve_project_variable_write_references(&analysis);
    assert_eq!(lexical.len(), 3);
    assert!(
        lexical
            .iter()
            .all(|resolution| resolution.status == DartLexicalBindingResolutionStatus::Resolved)
    );
    assert_eq!(
        lexical
            .iter()
            .map(|resolution| resolution.candidates[0].kind)
            .collect::<Vec<_>>(),
        [
            DartLexicalBindingKind::Parameter,
            DartLexicalBindingKind::LocalVariable,
            DartLexicalBindingKind::Parameter,
        ]
    );
    assert_eq!(
        lexical
            .iter()
            .map(|resolution| resolution.query.byte_offset)
            .collect::<Vec<_>>(),
        writes
            .iter()
            .map(|reference| reference.span.byte_start)
            .collect::<Vec<_>>()
    );

    let namespace = resolve_project_identifier_references(&analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        !matches!(
            resolution.reference.kind,
            DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
        )
    }));
}
