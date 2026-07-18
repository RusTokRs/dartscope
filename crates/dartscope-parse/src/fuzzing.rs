//! Internal fuzzing entry points.
//!
//! This module is intentionally available only through the opt-in `fuzzing` feature. It exposes
//! bounded harnesses over private parser stages without making their intermediate models part of the
//! supported public API.

use dartscope_core::SourceSpan;

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
    use super::{exercise_directives, exercise_graphql, exercise_lexical_masking};

    #[test]
    fn bridges_private_parser_stages() {
        let source = "import 'src/a.dart';\nconst query = r'''query A { viewer { id } }''';";
        exercise_lexical_masking(source);
        exercise_directives(source);
        exercise_graphql(source);
    }
}
