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
        let Some(callable) = enclosing_callable_declaration(analysis, token_start) else {
            continue;
        };
        let Some(owner_symbol_id) = callable.parent_symbol_id.clone() else {
            continue;
        };

        if direct_this_operand_ends_at(masked_source, token_end) {
            if let Some((operator, operator_start, operator_end)) =
                unary_operator_before(masked_source, token_start)
            {
                references.push(operator_reference(
                    source,
                    analysis,
                    callable,
                    owner_symbol_id.clone(),
                    operator,
                    operator_start,
                    operator_end,
                ));
            }
        }

        let operator_start = skip_whitespace(bytes, token_end);
        if expression_starts_at(masked_source, token_start) {
            if let Some((operator, operator_end)) =
                binary_operator_at(masked_source, operator_start)
            {
                references.push(operator_reference(
                    source,
                    analysis,
                    callable,
                    owner_symbol_id.clone(),
                    operator,
                    operator_start,
                    operator_end,
                ));
                at = operator_end;
                continue;
            }
        }
        if let Some((operator, anchor_end)) = index_operator_at(masked_source, operator_start) {
            references.push(operator_reference(
                source,
                analysis,
                callable,
                owner_symbol_id,
                operator,
                operator_start,
                anchor_end,
            ));
        }
    }
    references
}

fn operator_reference(
    source: &str,
    analysis: &DartFileAnalysis,
    callable: &DartDeclaration,
    owner_symbol_id: String,
    operator: &str,
    operator_start: usize,
    operator_end: usize,
) -> DartIdentifierReference {
    DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: operator.to_string(),
        prefix: Some(owner_symbol_id),
        kind: DartIdentifierReferenceKind::MemberOperatorInvocationInstance,
        confidence: Confidence::High,
        enclosing_symbol_id: callable.symbol_id.clone(),
        span: span_for_byte_range(source, operator_start, operator_end),
    }
}

fn unary_operator_before(
    source: &str,
    token_start: usize,
) -> Option<(&'static str, usize, usize)> {
    let bytes = source.as_bytes();
    let operator_end = skip_whitespace_back(bytes, token_start);
    let operator_start = operator_end.checked_sub(1)?;
    let operator = match bytes.get(operator_start) {
        Some(b'-') => "-",
        Some(b'~') => "~",
        _ => return None,
    };
    if operator == "-" && operator_start > 0 && bytes.get(operator_start - 1) == Some(&b'-') {
        return None;
    }
    expression_starts_at(source, operator_start)
        .then_some((operator, operator_start, operator_end))
}

fn direct_this_operand_ends_at(source: &str, token_end: usize) -> bool {
    let next = skip_whitespace(source.as_bytes(), token_end);
    !matches!(
        source.as_bytes().get(next),
        Some(b'.' | b'[' | b'(' | b'?')
    )
}

fn expression_starts_at(source: &str, start: usize) -> bool {
    let before = source.get(..start).unwrap_or_default().trim_end();
    if before.is_empty() {
        return true;
    }
    if ["return", "throw", "yield", "case"].iter().any(|keyword| {
        before.ends_with(keyword)
            && before
                .as_bytes()
                .get(before.len().saturating_sub(keyword.len() + 1))
                .is_none_or(|byte| !is_identifier_continue(*byte))
    }) {
        return true;
    }
    before.ends_with("=>")
        || before.as_bytes().last().is_some_and(|byte| {
            matches!(
                byte,
                b'(' | b'[' | b'{' | b',' | b':' | b';' | b'=' | b'?' | b'!'
            )
        })
}

fn index_operator_at(source: &str, start: usize) -> Option<(&'static str, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    let mut at = start;
    let close = loop {
        match bytes.get(at)? {
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break at;
                }
            }
            _ => {}
        }
        at += 1;
    };
    let after = skip_whitespace(bytes, close + 1);
    if compound_index_assignment_at(source, after) {
        return None;
    }
    let operator = if bytes.get(after) == Some(&b'=')
        && bytes.get(after + 1) != Some(&b'=')
        && bytes.get(after + 1) != Some(&b'>')
    {
        "[]="
    } else {
        "[]"
    };
    Some((operator, start + 1))
}

fn compound_index_assignment_at(source: &str, start: usize) -> bool {
    [
        ">>>=", "<<=", ">>=", "??=", "+=", "-=", "*=", "/=", "~/=", "%=", "&=", "|=", "^=", "++",
        "--",
    ]
    .iter()
    .any(|operator| {
        source
            .get(start..)
            .is_some_and(|rest| rest.starts_with(operator))
    })
}

fn binary_operator_at(source: &str, start: usize) -> Option<(&'static str, usize)> {
    const OPERATORS: [&str; 17] = [
        ">>>", "<<", ">>", "<=", ">=", "==", "~/", "+", "-", "/", "*", "%", "|", "^", "&", "<",
        ">",
    ];
    for operator in OPERATORS {
        if !source.get(start..)?.starts_with(operator) {
            continue;
        }
        let end = start + operator.len();
        if source.as_bytes().get(end) == Some(&b'=')
            && !matches!(operator, "<=" | ">=" | "==")
        {
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

fn skip_whitespace_back(bytes: &[u8], mut at: usize) -> usize {
    while at > 0 && bytes.get(at - 1).is_some_and(u8::is_ascii_whitespace) {
        at -= 1;
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
