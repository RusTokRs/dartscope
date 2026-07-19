use dartscope_core::{DartFileInput, DartProjectInput, DartSymbolResolutionStatus};
use dartscope_index::resolve_project_identifier_references;
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_only_invocations_not_shadowed_by_a_visible_local() {
    let source = r#"
void target() {}

void run() {
  target();
  {
    final target = callback;
    target();
  }
  target();
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));

    assert_eq!(analysis.references.len(), 2);
    let resolved = resolve_project_identifier_references(&analysis);
    assert_eq!(resolved.resolutions.len(), 2);
    for resolution in &resolved.resolutions {
        assert_eq!(resolution.reference.name, "target");
        assert_eq!(resolution.status, DartSymbolResolutionStatus::Resolved);
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].declaration_path, "lib/main.dart");
        assert_eq!(resolution.candidates[0].name, "target");
    }

    let invocation_offsets: Vec<_> = resolved
        .resolutions
        .iter()
        .map(|resolution| resolution.reference.span.byte_start)
        .collect();
    let first = source.find("target();").expect("first invocation");
    let last = source.rfind("target();").expect("last invocation");
    assert_eq!(invocation_offsets, [first, last]);
}
