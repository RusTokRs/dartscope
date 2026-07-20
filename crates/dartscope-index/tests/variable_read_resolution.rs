use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_identifier_references, resolve_project_variable_read_references,
};
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_variable_reads_only_through_parser_produced_binding_intervals() {
    let source = r#"
void consume(Object? value) {}

void run(int value) {
  consume(value);
  {
    final value = 1;
    consume(value);
  }
  consume(value);
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));
    let reads: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableRead)
        .collect();
    assert_eq!(reads.len(), 3);

    let lexical = resolve_project_variable_read_references(&analysis);
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
        reads
            .iter()
            .map(|reference| reference.span.byte_start)
            .collect::<Vec<_>>()
    );

    let namespace = resolve_project_identifier_references(&analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        resolution.reference.kind != DartIdentifierReferenceKind::VariableRead
    }));
}
