use dartscope_core::{
    DartDeclarationKind, DartFileInput, DartLexicalBindingKind, DartLexicalBindingQuery,
    DartLexicalBindingResolutionStatus, DartProjectInput,
};
use dartscope_index::resolve_project_lexical_binding;
use dartscope_parse::analyze_project_with_references;

#[test]
fn selects_the_most_specific_visible_binding_without_reparsing_source() {
    let source = r#"
void run(int value) {
  use(value);
  {
    final value = 1;
    use(value);
  }
  use(value);
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));
    let owner_id = analysis.project.files[0]
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind == DartDeclarationKind::Function && declaration.name == "run"
        })
        .and_then(|declaration| declaration.symbol_id.clone())
        .expect("run symbol");
    let uses: Vec<_> = source
        .match_indices("use(value)")
        .map(|(at, _)| at + 4)
        .collect();
    assert_eq!(uses.len(), 3);

    let first = resolve_project_lexical_binding(
        &analysis,
        DartLexicalBindingQuery::new("lib/main.dart", "value", uses[0])
            .with_enclosing_symbol_id(owner_id.clone()),
    );
    assert_eq!(first.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(first.candidates[0].kind, DartLexicalBindingKind::Parameter);

    let inner = resolve_project_lexical_binding(
        &analysis,
        DartLexicalBindingQuery::new("lib/main.dart", "value", uses[1])
            .with_enclosing_symbol_id(owner_id.clone()),
    );
    assert_eq!(inner.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(
        inner.candidates[0].kind,
        DartLexicalBindingKind::LocalVariable
    );

    let after = resolve_project_lexical_binding(
        &analysis,
        DartLexicalBindingQuery::new("lib/main.dart", "value", uses[2])
            .with_enclosing_symbol_id(owner_id),
    );
    assert_eq!(after.status, DartLexicalBindingResolutionStatus::Resolved);
    assert_eq!(after.candidates[0].kind, DartLexicalBindingKind::Parameter);

    let missing = resolve_project_lexical_binding(
        &analysis,
        DartLexicalBindingQuery::new("lib/main.dart", "missing", uses[0]),
    );
    assert_eq!(missing.status, DartLexicalBindingResolutionStatus::Missing);

    let missing_file = resolve_project_lexical_binding(
        &analysis,
        DartLexicalBindingQuery::new("lib/absent.dart", "value", uses[0]),
    );
    assert_eq!(
        missing_file.status,
        DartLexicalBindingResolutionStatus::SourceFileMissing
    );
}
