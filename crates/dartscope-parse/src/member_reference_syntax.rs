use dartscope_core::{DartDeclaration, DartDeclarationKind, SourceSpan};

pub(crate) fn declaration_name_range(
    masked_source: &str,
    declaration: &DartDeclaration,
) -> Option<(usize, usize)> {
    let span = declaration_span(declaration);
    let header_end = declaration_header_end(masked_source, span);
    match declaration.kind {
        DartDeclarationKind::Method => {
            method_name_range(masked_source, span.byte_start, header_end)
        }
        DartDeclarationKind::Getter => {
            name_after_keyword(masked_source, span.byte_start, header_end, "get")
        }
        DartDeclarationKind::Setter => {
            name_after_keyword(masked_source, span.byte_start, header_end, "set")
        }
        DartDeclarationKind::Field => field_name_range(
            masked_source,
            span.byte_start,
            header_end,
            &declaration.name,
        ),
        DartDeclarationKind::Operator => operator_name_range(
            masked_source,
            span.byte_start,
            header_end,
            &declaration.name,
        ),
        _ => identifier_name_range(
            masked_source,
            span.byte_start,
            header_end,
            &declaration.name,
        ),
    }
}

pub(crate) fn declaration_is_static(
    masked_source: &str,
    declaration: &DartDeclaration,
    name_start: usize,
) -> bool {
    let span = declaration_span(declaration);
    let bytes = masked_source.as_bytes();
    let mut at = span.byte_start;
    while at < name_start.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let end = identifier_end(bytes, at);
        if masked_source.get(at..end) == Some("static") {
            return true;
        }
        at = end;
    }
    false
}

pub(crate) fn looks_like_type_name(value: &str) -> bool {
    let name = value.trim_start_matches('_');
    !name.is_empty() && name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
}

pub(crate) fn declaration_span(declaration: &DartDeclaration) -> &SourceSpan {
    declaration
        .declaration_span
        .as_ref()
        .unwrap_or(&declaration.span)
}

fn method_name_range(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let paren = (start..end.min(bytes.len())).find(|index| bytes[*index] == b'(')?;
    last_identifier_range(source, start, paren)
}

fn name_after_keyword(
    source: &str,
    start: usize,
    end: usize,
    keyword: &str,
) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let token_end = identifier_end(bytes, at);
        if source.get(at..token_end) == Some(keyword) {
            let name_start = skip_whitespace(bytes, token_end);
            if name_start < end
                && bytes
                    .get(name_start)
                    .is_some_and(|byte| is_identifier_start(*byte))
            {
                return Some((name_start, identifier_end(bytes, name_start)));
            }
            return None;
        }
        at = token_end;
    }
    None
}

fn field_name_range(source: &str, start: usize, end: usize, name: &str) -> Option<(usize, usize)> {
    for (segment_start, segment_end) in top_level_segments(source, start, end) {
        let left_end =
            top_level_assignment(source, segment_start, segment_end).unwrap_or(segment_end);
        let Some(range) = last_identifier_range(source, segment_start, left_end) else {
            continue;
        };
        if source.get(range.0..range.1) == Some(name) {
            return Some(range);
        }
    }
    None
}

fn operator_name_range(
    source: &str,
    start: usize,
    end: usize,
    name: &str,
) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let token_end = identifier_end(bytes, at);
        if source.get(at..token_end) == Some("operator") {
            let operator_start = skip_whitespace(bytes, token_end);
            let operator_end = operator_start.checked_add(name.len())?;
            return (operator_end <= end && source.get(operator_start..operator_end) == Some(name))
                .then_some((operator_start, operator_end));
        }
        at = token_end;
    }
    None
}

fn identifier_name_range(
    source: &str,
    start: usize,
    end: usize,
    name: &str,
) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let token_end = identifier_end(bytes, at);
        if source.get(at..token_end) == Some(name) {
            return Some((at, token_end));
        }
        at = token_end;
    }
    None
}

fn last_identifier_range(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut at = start;
    let mut found = None;
    while at < end.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let token_end = identifier_end(bytes, at);
        found = Some((at, token_end));
        at = token_end;
    }
    found
}

fn top_level_segments(source: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut segments = Vec::new();
    let mut segment_start = start;
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
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                segments.push((segment_start, at));
                segment_start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    segments.push((segment_start, end.min(bytes.len())));
    segments
}

fn top_level_assignment(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'=' if parens == 0
                && brackets == 0
                && braces == 0
                && bytes.get(at + 1) != Some(&b'=')
                && bytes.get(at + 1) != Some(&b'>')
                && !matches!(
                    bytes.get(at.wrapping_sub(1)),
                    Some(b'=' | b'!' | b'<' | b'>')
                ) =>
            {
                return Some(at);
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut at = span.byte_start;
    while at < span.byte_end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => return at,
            b'=' if parens == 0 && brackets == 0 && bytes.get(at + 1) == Some(&b'>') => {
                return at;
            }
            _ => {}
        }
        at += 1;
    }
    span.byte_end.min(bytes.len())
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
