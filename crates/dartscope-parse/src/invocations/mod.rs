//! Parser-independent invocation facts for the conservative backend.

mod arguments;
mod scanner;

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartInvocation, DartInvocationArgument, SourceSpan,
};

use self::arguments::invocation_arguments;
use self::scanner::{CallCandidate, scan_call_candidates};
use crate::source_lines::{line_span_for_byte, span_for_byte_range};

pub(crate) fn collect_invocations(
    source: &str,
    masked_source: &str,
    declarations: &[DartDeclaration],
) -> Vec<DartInvocation> {
    let mut invocations: Vec<_> = scan_call_candidates(masked_source)
        .into_iter()
        .filter(|candidate| !is_declaration_header_call(candidate, masked_source, declarations))
        .map(|candidate| invocation_from_candidate(source, masked_source, declarations, candidate))
        .collect();
    invocations.sort_by(|left, right| {
        (left.span.byte_start, left.span.byte_end, &left.target).cmp(&(
            right.span.byte_start,
            right.span.byte_end,
            &right.target,
        ))
    });
    invocations.dedup_by(|left, right| {
        left.span.byte_start == right.span.byte_start
            && left.span.byte_end == right.span.byte_end
            && left.target == right.target
    });
    invocations
}

fn invocation_from_candidate(
    source: &str,
    masked_source: &str,
    declarations: &[DartDeclaration],
    candidate: CallCandidate,
) -> DartInvocation {
    let arguments: Vec<DartInvocationArgument> =
        invocation_arguments(source, masked_source, candidate.open + 1, candidate.close);
    DartInvocation {
        target: candidate.target,
        arguments,
        result_members: candidate.result_members,
        enclosing_symbol_id: enclosing_symbol_id(candidate.start, declarations),
        span: span_for_byte_range(source, candidate.start, candidate.end),
        source_line_span: line_span_for_byte(source, candidate.start),
    }
}

fn enclosing_symbol_id(at: usize, declarations: &[DartDeclaration]) -> Option<String> {
    declarations
        .iter()
        .filter(|declaration| is_callable_kind(declaration.kind))
        .filter_map(|declaration| {
            let span = declaration.declaration_span.as_ref()?;
            (span.byte_start <= at && at < span.byte_end).then_some((
                span.byte_end.saturating_sub(span.byte_start),
                declaration.symbol_id.as_ref(),
            ))
        })
        .min_by_key(|(length, _)| *length)
        .and_then(|(_, symbol_id)| symbol_id.cloned())
}

fn is_declaration_header_call(
    candidate: &CallCandidate,
    masked_source: &str,
    declarations: &[DartDeclaration],
) -> bool {
    declarations.iter().any(|declaration| {
        let Some(span) = declaration.declaration_span.as_ref() else {
            return false;
        };
        if candidate.start < span.byte_start || candidate.start >= span.byte_end {
            return false;
        }
        let header_end = declaration_header_end(masked_source, span);
        if candidate.start >= header_end {
            return false;
        }
        candidate.target == declaration.name
    })
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut index = span.byte_start;
    while index < span.byte_end.min(bytes.len()) {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => return index,
            b'=' if parens == 0 && brackets == 0 && bytes.get(index + 1) == Some(&b'>') => {
                return index;
            }
            _ => {}
        }
        index += 1;
    }
    span.byte_end
}

fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}
