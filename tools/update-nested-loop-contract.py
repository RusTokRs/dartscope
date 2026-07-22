from pathlib import Path

path = Path("crates/dartscope-index/tests/single_statement_for_resolution.rs")
source = path.read_text(encoding="utf-8")
start_marker = "#[test]\nfn keeps_nested_control_deferred_and_namespace_filtered()"
start = source.index(start_marker)
end = source.index("\nfn assert_same_resolution(", start)
replacement = r'''#[test]
fn resolves_inner_nested_loop_while_preserving_outer_deferment() {
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
    ] {
        assert!(
            reads
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
        assert!(
            writes
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
    }
    for offset in [
        occurrence("nested < 1", "nested"),
        occurrence("consume(nested)", "nested"),
    ] {
        assert_resolution(
            &reads,
            offset,
            DartLexicalBindingKind::LocalVariable,
            "/for_variable:nested@",
        );
        assert!(
            writes
                .iter()
                .all(|resolution| resolution.query.byte_offset != offset)
        );
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
'''
source = source[:start] + replacement + source[end:]
path.write_text(source, encoding="utf-8")
