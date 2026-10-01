//! Internal fuzzing entry points.
//!
//! This module is intentionally available only through the opt-in `fuzzing` feature. It exposes
//! bounded harnesses over private parser stages without making their intermediate models part of the
//! supported public API.

use dartscope_core::{DartFileInput, DartFileReferenceAnalysis, SourceSpan};

use crate::graphql::{extract_graphql_operation_uses, extract_graphql_operations};
use crate::lexical::mask_non_code;
use crate::namespace::extract_namespace_directives;

/// Exercises lexical masking and its byte-preservation invariant.
pub fn exercise_lexical_masking(source: &str) {
    let mask = mask_non_code(source);
    assert_eq!(mask.code.len(), source.len());

    for (original, masked) in source.bytes().zip(mask.code.bytes()) {
        if matches!(original, b'\n' | b'\r') {
            assert_eq!(masked, original);
        }
    }

    for diagnostic in &mask.diagnostics {
        if let Some(span) = diagnostic.span.as_ref() {
            assert_span(source, span);
        }
    }
}

/// Exercises import/export directive extraction over the exact lexical mask used by file analysis.
pub fn exercise_directives(source: &str) {
    let mask = mask_non_code(source);
    let (imports, exports, diagnostics) = extract_namespace_directives(source, &mask.code);

    for import in imports {
        assert_span(source, &import.span);
    }
    for export in exports {
        assert_span(source, &export.span);
    }
    for diagnostic in diagnostics {
        if let Some(span) = diagnostic.span.as_ref() {
            assert_span(source, span);
        }
    }
}

/// Exercises GraphQL declaration and invocation extraction over the lexical mask.
pub fn exercise_graphql(source: &str) {
    let mask = mask_non_code(source);
    let operations = extract_graphql_operations(source, &mask.code);
    let uses = extract_graphql_operation_uses(source, &mask.code);

    for operation in operations {
        assert_span(source, &operation.span);
    }
    for operation_use in uses {
        assert_span(source, &operation_use.span);
    }
}

/// Exercises the whole file analysis with its reference passes on arbitrary text.
///
/// Nothing may panic, two runs over the same text must agree, and every span the analysis reports
/// must lie inside the text, on character boundaries, with positive lines and columns.
pub fn exercise_file_analysis(source: &str) {
    let input = DartFileInput::new("lib/fuzz.dart", source);
    let analysis = crate::analyze_file_with_references(input.clone());
    assert_eq!(analysis, crate::analyze_file_with_references(input));
    for span in all_spans(&analysis) {
        assert!(span.byte_start <= span.byte_end, "{span:?}");
        assert!(span.byte_end <= source.len(), "{span:?}");
        assert!(source.is_char_boundary(span.byte_start), "{span:?}");
        assert!(source.is_char_boundary(span.byte_end), "{span:?}");
        assert!(span.start_line >= 1 && span.start_column >= 1, "{span:?}");
        assert!(span.end_line >= span.start_line, "{span:?}");
    }
}

/// Every span of a file analysis: declarations, invocations and their arguments, directives, string
/// constants, diagnostics, references and lexical bindings.
fn all_spans(analysis: &DartFileReferenceAnalysis) -> Vec<&SourceSpan> {
    let file = &analysis.file;
    let mut spans = Vec::new();
    for declaration in &file.declarations {
        spans.push(&declaration.span);
        spans.extend(declaration.declaration_span.iter());
    }
    for invocation in &file.invocations {
        spans.push(&invocation.span);
        spans.push(&invocation.source_line_span);
        spans.extend(invocation.arguments.iter().map(|argument| &argument.span));
    }
    spans.extend(file.imports.iter().map(|import| &import.span));
    spans.extend(file.exports.iter().map(|export| &export.span));
    spans.extend(file.parts.iter().map(|part| &part.span));
    spans.extend(file.string_constants.iter().map(|constant| &constant.span));
    spans.extend(
        file.diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.span.as_ref()),
    );
    spans.extend(analysis.references.iter().map(|reference| &reference.span));
    for binding in &analysis.bindings {
        spans.push(&binding.declaration_span);
        spans.push(&binding.scope_span);
    }
    spans
}

fn assert_span(source: &str, span: &SourceSpan) {
    assert!(span.byte_start <= span.byte_end);
    assert!(span.byte_end <= source.len());
    assert!(span.start_line >= 1);
    assert!(span.end_line >= span.start_line);
    assert!(span.start_column >= 1);
    assert!(span.end_column >= 1);
}

#[cfg(test)]
mod tests {
    use super::{
        exercise_directives, exercise_file_analysis, exercise_graphql, exercise_lexical_masking,
    };

    #[test]
    fn bridges_private_parser_stages() {
        let source = "import 'src/a.dart';\nconst query = r'''query A { viewer { id } }''';";
        exercise_lexical_masking(source);
        exercise_directives(source);
        exercise_graphql(source);
        exercise_file_analysis(source);
    }

    #[test]
    fn the_file_bridge_accepts_valid_broken_and_non_ascii_text() {
        for source in [
            "",
            "\u{feff}class A extends B { void m() { var x = f(g(1), y: [2]); } }\n",
            "class A { void m() { for (var i = 0; i < 3; i++) g(i, (é) => é); } }",
            "@A(@B(\nvoid f(((<<{{[[ 'x",
            "import 'a.dart'\nimport 'b.dart' as b;\nvoid main() { b.run(); }\r\n",
        ] {
            exercise_file_analysis(source);
        }
    }
}
