from pathlib import Path

path = Path("crates/dartscope-parse/tests/single_statement_for.rs")
source = path.read_text(encoding="utf-8")
start_marker = "#[test]\nfn defers_nested_control_and_preserves_following_boundary()"
start = source.index(start_marker)
end = source.index("\nfn occurrence(", start)
replacement = r'''#[test]
fn supports_nested_control_and_preserves_following_boundary() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/single_statement_for.dart", SOURCE));

    let deferred = analysis
        .bindings
        .iter()
        .find(|binding| {
            binding.name == "deferred" && binding.symbol_id.contains("/for_variable:deferred@")
        })
        .expect("outer nested-loop binding");
    let nested = analysis
        .bindings
        .iter()
        .find(|binding| {
            binding.name == "nested" && binding.symbol_id.contains("/for_variable:nested@")
        })
        .expect("inner nested-loop binding");
    for offset in [
        occurrence("deferred < 1", "deferred"),
        occurrence("var nested = deferred", "deferred"),
        occurrence("nested++) consume", "nested"),
        occurrence("consume(nested)", "nested"),
    ] {
        assert!(
            deferred.scope_span.byte_start <= offset && offset < deferred.scope_span.byte_end
                || nested.scope_span.byte_start <= offset && offset < nested.scope_span.byte_end
        );
    }

    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            occurrence("var deferred = seed", "seed")
        ),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    for offset in [
        occurrence("deferred < 1", "deferred"),
        occurrence("var nested = deferred", "deferred"),
        occurrence("nested < 1", "nested"),
        occurrence("consume(nested)", "nested"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }
    for offset in [
        occurrence("deferred++)", "deferred"),
        occurrence("nested++) consume", "nested"),
    ] {
        assert_eq!(
            variable_kinds_at(&analysis.references, offset),
            vec![
                DartIdentifierReferenceKind::VariableRead,
                DartIdentifierReferenceKind::VariableWrite,
            ]
        );
    }

    assert_eq!(
        variable_kinds_at(
            &analysis.references,
            last_occurrence("consume(seed)", "seed")
        ),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
    assert!(deferred.scope_span.byte_end <= last_occurrence("consume(seed)", "seed"));
    assert!(nested.scope_span.byte_end <= last_occurrence("consume(seed)", "seed"));
}
'''
source = source[:start] + replacement + source[end:]
path.write_text(source, encoding="utf-8")
