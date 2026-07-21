use dartscope_core::{
    Confidence, DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind,
};

use crate::member_reference_syntax::{declaration_name_range, declaration_span};
use crate::source_lines::span_for_byte_range;

pub(crate) fn collect_operator_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    let mut references = operator_declaration_references(source, masked_source, analysis);
    references.extend(operator_invocation_references(
        source,
        masked_source,
        analysis,
    ));
    references.sort_by(|left, right| {
        (
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
    references.dedup();
    references
}

fn operator_declaration_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DartDeclarationKind::Operator)
        .filter_map(|declaration| {
            let owner_symbol_id = declaration.parent_symbol_id.clone()?;
            let (name_start, name_end) = declaration_name_range(masked_source, declaration)?;
            Some(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: declaration.name.clone(),
                prefix: Some(owner_symbol_id),
                kind: DartIdentifierReferenceKind::MemberOperatorDeclaration,
                confidence: Confidence::High,
                enclosing_symbol_id: None,
                span: span_for_byte_range(source, name_start, name_end),
            })
        })
        .collect()
}

fn operator_invocation_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    let bytes = masked_source.as_bytes();
    let mut references = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let token_start = at;
        let token_end = identifier_end(bytes, token_start);
        at = token_end;
        if masked_source.get(token_start..token_end) != Some("this") {
            continue;
        }
        let operator_start = skip_whitespace(bytes, token_end);
        let Some((operator, operator_end)) = binary_operator_at(masked_source, operator_start)
        else {
            continue;
        };
        let callable = enclosing_callable_declaration(analysis, token_start);
        let Some(callable) = callable else {
            continue;
        };
        let Some(owner_symbol_id) = callable.parent_symbol_id.clone() else {
            continue;
        };
        references.push(DartIdentifierReference {
            source_path: analysis.path.clone(),
            name: operator.to_string(),
            prefix: Some(owner_symbol_id),
            kind: DartIdentifierReferenceKind::MemberOperatorInvocationInstance,
            confidence: Confidence::High,
            enclosing_symbol_id: callable.symbol_id.clone(),
            span: span_for_byte_range(source, operator_start, operator_end),
        });
        at = operator_end;
    }
    references
}

fn binary_operator_at(source: &str, start: usize) -> Option<(&'static str, usize)> {
    const OPERATORS: [&str; 17] = [
        ">>>", "<<", ">>", "<=", ">=", "==", "~/", "+", "-", "/", "*", "%", "|", "^", "&", "<", ">",
    ];
    for operator in OPERATORS {
        if !source.get(start..)?.starts_with(operator) {
            continue;
        }
        let end = start + operator.len();
        if source.as_bytes().get(end) == Some(&b'=') && !matches!(operator, "<=" | ">=" | "==") {
            continue;
        }
        if matches!(operator, "+" | "-")
            && source
                .as_bytes()
                .get(end)
                .is_some_and(|byte| *byte == operator.as_bytes()[0])
        {
            continue;
        }
        return Some((operator, end));
    }
    None
}

fn enclosing_callable_declaration(
    analysis: &DartFileAnalysis,
    byte_offset: usize,
) -> Option<&DartDeclaration> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| {
            is_callable_kind(declaration.kind)
                && declaration.parent_symbol_id.is_some()
                && declaration_span(declaration).byte_start <= byte_offset
                && byte_offset < declaration_span(declaration).byte_end
        })
        .min_by_key(|declaration| {
            declaration_span(declaration)
                .byte_end
                .saturating_sub(declaration_span(declaration).byte_start)
        })
}

fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn identifier_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
