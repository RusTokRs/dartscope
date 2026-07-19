use std::collections::HashSet;

use dartscope_core::{
    Confidence, DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartInvocation, SourceSpan,
};

use crate::source_lines::span_for_byte_range;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy)]
struct ClauseRange {
    start: usize,
    end: usize,
}

pub(super) fn collect_typed_identifier_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    let import_prefixes: HashSet<String> = analysis
        .imports
        .iter()
        .filter_map(|import| import.prefix.clone())
        .collect();
    let mut references = Vec::new();

    for declaration in analysis
        .declarations
        .iter()
        .filter(|declaration| is_type_declaration_kind(declaration.kind))
    {
        collect_nominal_type_clause_references(
            source,
            masked_source,
            analysis,
            declaration,
            &import_prefixes,
            &mut references,
        );
    }

    for invocation in &analysis.invocations {
        if let Some(reference) = constructor_target_reference(
            source,
            masked_source,
            analysis,
            invocation,
            &import_prefixes,
        ) {
            references.push(reference);
        }
    }

    references
}

fn collect_nominal_type_clause_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    import_prefixes: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    let header_end = declaration_header_end(masked_source, span);
    if header_end <= span.byte_start {
        return;
    }
    let type_parameters = type_parameter_names(masked_source, span.byte_start, header_end);

    for range in clause_ranges(masked_source, span.byte_start, header_end, declaration.kind) {
        collect_clause_type_roots(
            source,
            masked_source,
            analysis,
            declaration,
            range,
            import_prefixes,
            &type_parameters,
            references,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_clause_type_roots(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    range: ClauseRange,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    let bytes = masked_source.as_bytes();
    let mut at = range.start;
    while at < range.end.min(bytes.len()) {
        at = skip_whitespace_and_commas(bytes, at, range.end);
        let Some(first) = identifier_at(masked_source, at) else {
            at += 1;
            continue;
        };

        let dot = skip_whitespace(bytes, first.end);
        let (token, prefix, after) = if dot < range.end && bytes.get(dot) == Some(&b'.') {
            let member_start = skip_whitespace(bytes, dot + 1);
            let Some(member) = identifier_at(masked_source, member_start) else {
                at = skip_type_expression(masked_source, first.end, range.end);
                continue;
            };
            if !import_prefixes.contains(first.text) {
                at = skip_type_expression(masked_source, member.end, range.end);
                continue;
            }
            (member, Some(first.text.to_string()), member.end)
        } else {
            (first, None, first.end)
        };

        if !type_parameters.contains(token.text) {
            references.push(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: token.text.to_string(),
                prefix: prefix.clone(),
                kind: DartIdentifierReferenceKind::TypeAnnotation,
                confidence: if prefix.is_some() {
                    Confidence::High
                } else {
                    Confidence::Medium
                },
                enclosing_symbol_id: declaration.symbol_id.clone(),
                span: span_for_byte_range(source, token.start, token.end),
            });
        }

        at = skip_type_expression(masked_source, after, range.end);
    }
}

fn constructor_target_reference(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    invocation: &DartInvocation,
    import_prefixes: &HashSet<String>,
) -> Option<DartIdentifierReference> {
    if !has_constructor_keyword(masked_source, invocation.span.byte_start) {
        return None;
    }
    let tokens = invocation_target_tokens(masked_source, invocation.span.byte_start)?;
    let (token, prefix) = match tokens.as_slice() {
        [single] => (*single, None),
        [first, second] if import_prefixes.contains(first.text) => {
            (*second, Some(first.text.to_string()))
        }
        [first, _named] => (*first, None),
        [first, second, _named] if import_prefixes.contains(first.text) => {
            (*second, Some(first.text.to_string()))
        }
        _ => return None,
    };

    Some(DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: token.text.to_string(),
        prefix: prefix.clone(),
        kind: DartIdentifierReferenceKind::ConstructorTarget,
        confidence: if prefix.is_some() {
            Confidence::High
        } else {
            Confidence::Medium
        },
        enclosing_symbol_id: invocation.enclosing_symbol_id.clone(),
        span: span_for_byte_range(source, token.start, token.end),
    })
}

fn clause_ranges(
    source: &str,
    start: usize,
    end: usize,
    kind: DartDeclarationKind,
) -> Vec<ClauseRange> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut current = None;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut angles = 0usize;
    let mut at = start;

    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if parens == 0
                && brackets == 0
                && angles == 0
                && supports_clause(kind, token.text)
                && let Some(range_start) = current.replace(token.end)
            {
                ranges.push(ClauseRange {
                    start: range_start,
                    end: token.start,
                });
            }
            at = token.end;
            continue;
        }
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            _ => {}
        }
        at += 1;
    }

    if let Some(range_start) = current {
        ranges.push(ClauseRange {
            start: range_start,
            end,
        });
    }
    ranges
}

fn type_parameter_names(source: &str, start: usize, end: usize) -> HashSet<String> {
    let bytes = source.as_bytes();
    let mut at = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    while at < end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 => {
                return parse_type_parameter_block(source, at, end);
            }
            _ => {}
        }
        at += 1;
    }
    HashSet::new()
}

fn parse_type_parameter_block(source: &str, open: usize, end: usize) -> HashSet<String> {
    let bytes = source.as_bytes();
    let mut names = HashSet::new();
    let mut depth = 1usize;
    let mut expect_name = true;
    let mut at = open + 1;
    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if depth == 1 && expect_name {
                names.insert(token.text.to_string());
                expect_name = false;
            }
            at = token.end;
            continue;
        }
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => expect_name = true,
            _ => {}
        }
        at += 1;
    }
    names
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut angles = 0usize;
    let mut at = span.byte_start;
    while at < span.byte_end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 && angles == 0 => return at,
            b'=' if parens == 0
                && brackets == 0
                && angles == 0
                && bytes.get(at + 1) == Some(&b'>') =>
            {
                return at;
            }
            _ => {}
        }
        at += 1;
    }
    span.byte_end.min(bytes.len())
}

fn skip_type_expression(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                return at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    end
}

fn invocation_target_tokens(source: &str, start: usize) -> Option<Vec<IdentifierToken<'_>>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = start;
    loop {
        let token = identifier_at(source, at)?;
        tokens.push(token);
        at = skip_whitespace(bytes, token.end);
        if bytes.get(at) == Some(&b'<') {
            at = matching_angle(source, at)? + 1;
            at = skip_whitespace(bytes, at);
        }
        if bytes.get(at) != Some(&b'.') {
            break;
        }
        at = skip_whitespace(bytes, at + 1);
    }
    Some(tokens)
}

fn matching_angle(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 1usize;
    let mut at = open + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn has_constructor_keyword(source: &str, start: usize) -> bool {
    matches!(preceding_identifier(source, start), Some("new" | "const"))
}

fn preceding_identifier(source: &str, start: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut end = start.min(bytes.len());
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut begin = end;
    while begin > 0 && is_identifier_continue(bytes[begin - 1]) {
        begin -= 1;
    }
    (begin < end).then_some(&source[begin..end])
}

fn supports_clause(kind: DartDeclarationKind, keyword: &str) -> bool {
    match kind {
        DartDeclarationKind::Class => matches!(keyword, "extends" | "with" | "implements"),
        DartDeclarationKind::Mixin => matches!(keyword, "on" | "implements"),
        DartDeclarationKind::Enum => matches!(keyword, "with" | "implements"),
        DartDeclarationKind::Extension => keyword == "on",
        DartDeclarationKind::ExtensionType => keyword == "implements",
        _ => false,
    }
}

fn is_type_declaration_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

fn skip_whitespace_and_commas(bytes: &[u8], mut at: usize, end: usize) -> usize {
    while at < end
        && bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b',')
    {
        at += 1;
    }
    at
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn identifier_at(source: &str, start: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    if !bytes
        .get(start)
        .is_some_and(|byte| is_identifier_start(*byte))
    {
        return None;
    }
    let end = identifier_end(bytes, start);
    Some(IdentifierToken {
        text: &source[start..end],
        start,
        end,
    })
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
