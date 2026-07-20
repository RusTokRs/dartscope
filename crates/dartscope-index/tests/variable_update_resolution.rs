use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::{
    resolve_project_identifier_references, resolve_project_variable_read_references,
    resolve_project_variable_write_references,
};
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_combined_update_reads_and_writes_to_the_same_visible_binding() {
    let source = r#"
void run(int value, int other) {
  value += other;
  {
    var value = 0;
    value++;
  }
  --value;
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));
    let target_offsets = [
        source.find("value +=").expect("compound assignment"),
        source.find("value++").expect("postfix increment"),
        source.rfind("value;").expect("prefix decrement"),
    ];

    let reads: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| {
            reference.kind == DartIdentifierReferenceKind::VariableRead
                && reference.name == "value"
                && target_offsets.contains(&reference.span.byte_start)
        })
        .collect();
    let writes: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| {
            reference.kind == DartIdentifierReferenceKind::VariableWrite
                && reference.name == "value"
        })
        .collect();
    assert_eq!(
        reads
            .iter()
            .map(|reference| reference.span.byte_start)
            .collect::<Vec<_>>(),
        target_offsets
    );
    assert_eq!(
        writes
            .iter()
            .map(|reference| reference.span.byte_start)
            .collect::<Vec<_>>(),
        target_offsets
    );

    let read_resolutions: Vec<_> = resolve_project_variable_read_references(&analysis)
        .into_iter()
        .filter(|resolution| {
            resolution.query.name == "value"
                && target_offsets.contains(&resolution.query.byte_offset)
        })
        .collect();
    let write_resolutions = resolve_project_variable_write_references(&analysis);
    assert_eq!(read_resolutions.len(), 3);
    assert_eq!(write_resolutions.len(), 3);
    for resolution in read_resolutions.iter().chain(&write_resolutions) {
        assert_eq!(
            resolution.status,
            DartLexicalBindingResolutionStatus::Resolved
        );
        assert_eq!(resolution.candidates.len(), 1);
    }
    let expected_kinds = [
        DartLexicalBindingKind::Parameter,
        DartLexicalBindingKind::LocalVariable,
        DartLexicalBindingKind::Parameter,
    ];
    assert_eq!(
        read_resolutions
            .iter()
            .map(|resolution| resolution.candidates[0].kind)
            .collect::<Vec<_>>(),
        expected_kinds
    );
    assert_eq!(
        write_resolutions
            .iter()
            .map(|resolution| resolution.candidates[0].kind)
            .collect::<Vec<_>>(),
        expected_kinds
    );
    assert!(
        read_resolutions
            .iter()
            .zip(&write_resolutions)
            .all(|(read, write)| {
                read.query.byte_offset == write.query.byte_offset
                    && read.candidates[0].symbol_id == write.candidates[0].symbol_id
            })
    );

    let namespace = resolve_project_identifier_references(&analysis);
    assert!(namespace.resolutions.iter().all(|resolution| {
        !matches!(
            resolution.reference.kind,
            DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
        )
    }));
}
