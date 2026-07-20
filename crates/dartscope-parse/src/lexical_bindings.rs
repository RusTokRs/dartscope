use std::collections::HashMap;

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartLexicalBinding,
    DartLexicalBindingKind, SourceSpan,
};

use crate::source_lines::span_for_byte_range;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

pub(crate) fn collect_lexical_bindings(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartLexicalBinding> {
    let mut bindings = Vec::new();
    for declaration in analysis
        .declarations
        .iter()
        .filter(|declaration| supports_parameters(declaration.kind))
    {
        collect_parameter_bindings(source, masked_source, analysis, declaration, &mut bindings);
    }
    for declaration in analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DartDeclarationKind::LocalVariable)
    {
        collect_local_binding(source, masked_source, analysis, declaration, &mut bindings);
    }
    sort_lexical_bindings(&mut bindings);
    bindings.dedup_by(|left, right| {
        left.source_path == right.source_path
            && left.symbol_id == right.symbol_id
            && left.declaration_span.byte_start == right.declaration_span.byte_start
            && left.declaration_span.byte_end == right.declaration_span.byte_end
    });
    bindings
}

pub(crate) fn sort_lexical_bindings(bindings: &mut [DartLexicalBinding]) {
    bindings.sort_by(|left, right| {
        (
            &left.source_path,
            left.declaration_span.byte_start,
            left.declaration_span.byte_end,
            left.kind,
            &left.name,
            &left.symbol_id,
        )
            .cmp(&(
                &right.source_path,
                right.declaration_span.byte_start,
                right.declaration_span.byte_end,
                right.kind,
                &right.name,
                &right.symbol_id,
            ))
    });
}

fn collect_parameter_bindings(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    bindings: &mut Vec<DartLexicalBinding>,
) {
    let Some(owner_id) = declaration.symbol_id.as_deref() else {
        return;
    };
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    let header_end = declaration_header_end(masked_source, span);
    let Some((start, end, close)) =
        callable_parameter_range(masked_source, span.byte_start, header_end, declaration)
    else {
        return;
    };
    if close > span.byte_end {
        return;
    }
    let scope_span = span_for_byte_range(source, close + 1, span.byte_end);
    let mut occurrences = HashMap::new();
    collect_parameter_range(
        source,
        masked_source,
        analysis,
        owner_id,
        start,
        end,
        &scope_span,
        &mut occurrences,
        bindings,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_parameter_range(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    owner_id: &str,
    start: usize,
    end: usize,
    scope_span: &SourceSpan,
    occurrences: &mut HashMap<String, usize>,
    bindings: &mut Vec<DartLexicalBinding>,
) {
    for (segment_start, segment_end) in top_level_segments(masked_source, start, end) {
        let Some((trimmed_start, trimmed_end)) =
            trim_range(masked_source, segment_start, segment_end)
        else {
            continue;
        };
        let bytes = masked_source.as_bytes();
        if matches!(
            (
                bytes.get(trimmed_start),
                bytes.get(trimmed_end.saturating_sub(1))
            ),
            (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']'))
        ) {
            collect_parameter_range(
                source,
                masked_source,
                analysis,
                owner_id,
                trimmed_start + 1,
                trimmed_end - 1,
                scope_span,
                occurrences,
                bindings,
            );
            continue;
        }

        let declaration_end =
            top_level_assignment(masked_source, trimmed_start, trimmed_end).unwrap_or(trimmed_end);
        if contains_receiver_formal(masked_source, trimmed_start, declaration_end)
            || contains_top_level_pattern_delimiter(masked_source, trimmed_start, declaration_end)
        {
            continue;
        }
        let Some(name) = last_top_level_identifier(masked_source, trimmed_start, declaration_end)
        else {
            continue;
        };
        if !is_binding_name(name.text) {
            continue;
        }
        let occurrence = occurrences.entry(name.text.to_string()).or_insert(0);
        *occurrence += 1;
        let symbol_id = if *occurrence == 1 {
            format!("{owner_id}/parameter:{}", name.text)
        } else {
            format!("{owner_id}/parameter:{}#{}", name.text, *occurrence)
        };
        bindings.push(DartLexicalBinding {
            source_path: analysis.path.clone(),
            name: name.text.to_string(),
            kind: DartLexicalBindingKind::Parameter,
            symbol_id,
            enclosing_symbol_id: owner_id.to_string(),
            declaration_span: span_for_byte_range(source, name.start, name.end),
            scope_span: scope_span.clone(),
        });
    }
}

fn collect_local_binding(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    bindings: &mut Vec<DartLexicalBinding>,
) {
    let Some(owner_id) = declaration.parent_symbol_id.as_deref() else {
        return;
    };
    let Some(symbol_id) = declaration.symbol_id.as_deref() else {
        return;
    };
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    if is_deferred_control_binding(masked_source, span.byte_start, span.byte_end) {
        return;
    }
    let Some(owner) = analysis
        .declarations
        .iter()
        .find(|candidate| candidate.symbol_id.as_deref() == Some(owner_id))
    else {
        return;
    };
    let Some(token) = local_declarator_tokens(masked_source, span.byte_start, span.byte_end)
        .into_iter()
        .find(|token| token.text == declaration.name)
    else {
        return;
    };
    let Some(scope_end) = local_scope_end(masked_source, span.byte_start, owner) else {
        return;
    };
    if span.byte_end > scope_end {
        return;
    }
    bindings.push(DartLexicalBinding {
        source_path: analysis.path.clone(),
        name: declaration.name.clone(),
        kind: DartLexicalBindingKind::LocalVariable,
        symbol_id: symbol_id.to_string(),
        enclosing_symbol_id: owner_id.to_string(),
        declaration_span: span_for_byte_range(source, token.start, token.end),
        scope_span: span_for_byte_range(source, span.byte_end, scope_end),
    });
}

fn local_declarator_tokens(source: &str, start: usize, end: usize) -> Vec<IdentifierToken<'_>> {
    top_level_segments(source, start, end)
        .into_iter()
        .filter_map(|(segment_start, segment_end)| {
            let (segment_start, segment_end) = trim_range(source, segment_start, segment_end)?;
            let declaration_end =
                top_level_assignment(source, segment_start, segment_end).unwrap_or(segment_end);
            last_top_level_identifier(source, segment_start, declaration_end)
        })
        .filter(|token| is_binding_name(token.text))
        .collect()
}

fn local_scope_end(
    source: &str,
    declaration_start: usize,
    owner: &DartDeclaration,
) -> Option<usize> {
    let owner_span = owner.declaration_span.as_ref()?;
    let bytes = source.as_bytes();
    let mut blocks = Vec::new();
    let mut at = owner_span.byte_start;
    while at < declaration_start.min(owner_span.byte_end).min(bytes.len()) {
        match bytes[at] {
            b'{' => blocks.push(at),
            b'}' => {
                blocks.pop();
            }
            _ => {}
        }
        at += 1;
    }
    matching_delimiter(source, *blocks.last()?, owner_span.byte_end, b'{', b'}')
}

fn callable_parameter_range(
    source: &str,
    start: usize,
    end: usize,
    declaration: &DartDeclaration,
) -> Option<(usize, usize, usize)> {
    let open = match declaration.kind {
        DartDeclarationKind::Operator => {
            let operator = find_identifier_named(source, start, end, "operator")?;
            find_next_byte(source, operator.end, end, b'(')?
        }
        DartDeclarationKind::Constructor => {
            let name_end = find_qualified_name_end(source, start, end, &declaration.name)?;
            let open = skip_whitespace(source.as_bytes(), name_end);
            (source.as_bytes().get(open) == Some(&b'(')).then_some(open)?
        }
        _ => {
            let token = callable_name_token(source, start, end, declaration)?;
            let mut open = skip_whitespace(source.as_bytes(), token.end);
            if source.as_bytes().get(open) == Some(&b'<') {
                open = matching_delimiter(source, open, end, b'<', b'>')? + 1;
                open = skip_whitespace(source.as_bytes(), open);
            }
            (source.as_bytes().get(open) == Some(&b'(')).then_some(open)?
        }
    };
    let close = matching_delimiter(source, open, end, b'(', b')')?;
    Some((open + 1, close, close))
}

fn callable_name_token<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    declaration: &DartDeclaration,
) -> Option<IdentifierToken<'source>> {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end.min(bytes.len()) {
        let Some(token) = next_identifier(source, at, end) else {
            break;
        };
        at = token.end;
        if token.text != declaration.name {
            continue;
        }
        let mut next = skip_whitespace(bytes, token.end);
        if bytes.get(next) == Some(&b'<') {
            next = matching_delimiter(source, next, end, b'<', b'>')? + 1;
            next = skip_whitespace(bytes, next);
        }
        if bytes.get(next) == Some(&b'(') {
            return Some(token);
        }
    }
    None
}

fn find_qualified_name_end(source: &str, start: usize, end: usize, name: &str) -> Option<usize> {
    let haystack = source.get(start..end)?;
    for (offset, _) in haystack.match_indices(name) {
        let absolute = start + offset;
        let before_ok = absolute == start
            || source
                .as_bytes()
                .get(absolute.saturating_sub(1))
                .is_none_or(|byte| !is_identifier_continue(*byte));
        let after = absolute + name.len();
        let after_ok = source
            .as_bytes()
            .get(after)
            .is_none_or(|byte| !is_identifier_continue(*byte));
        if before_ok && after_ok {
            let open = skip_whitespace(source.as_bytes(), after);
            if source.as_bytes().get(open) == Some(&b'(') {
                return Some(after);
            }
        }
    }
    None
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
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                segments.push((segment_start, at));
                segment_start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    segments.push((segment_start, end));
    segments
}

fn top_level_assignment(source: &str, start: usize, end: usize) -> Option<usize> {
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
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b'=' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                return Some(at);
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn last_top_level_identifier(
    source: &str,
    start: usize,
    end: usize,
) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    let mut last = None;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if parens == 0 && brackets == 0 && braces == 0 && angles == 0 {
                last = Some(token);
            }
            at = token.end;
            continue;
        }
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            _ => {}
        }
        at += 1;
    }
    last
}

fn contains_receiver_formal(source: &str, start: usize, end: usize) -> bool {
    source
        .get(start..end)
        .is_some_and(|value| value.contains("this.") || value.contains("super."))
}

fn contains_top_level_pattern_delimiter(source: &str, start: usize, end: usize) -> bool {
    let Some(value) = source.get(start..end) else {
        return true;
    };
    let trimmed = value.trim_start();
    trimmed.starts_with('(') || trimmed.starts_with('[') || trimmed.starts_with('{')
}

fn is_deferred_control_binding(source: &str, start: usize, end: usize) -> bool {
    let Some(value) = source.get(start..end) else {
        return true;
    };
    let trimmed = value.trim_start();
    [
        "for ",
        "for(",
        "await for ",
        "await for(",
        "catch ",
        "catch(",
        "on ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn trim_range(source: &str, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    while end > start
        && bytes
            .get(end - 1)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b';')
    {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

fn find_identifier_named<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    name: &str,
) -> Option<IdentifierToken<'source>> {
    let mut at = start;
    while let Some(token) = next_identifier(source, at, end) {
        if token.text == name {
            return Some(token);
        }
        at = token.end;
    }
    None
}

fn next_identifier(source: &str, mut at: usize, end: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    while at < end.min(bytes.len()) && !is_identifier_start(bytes[at]) {
        at += 1;
    }
    identifier_at(source, at).filter(|token| token.end <= end)
}

fn find_next_byte(source: &str, mut at: usize, end: usize, target: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    while at < end.min(bytes.len()) {
        if bytes[at] == target {
            return Some(at);
        }
        at += 1;
    }
    None
}

fn matching_delimiter(
    source: &str,
    open: usize,
    end: usize,
    opening: u8,
    closing: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&opening) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = open + 1;
    while at < end.min(bytes.len()) {
        if bytes[at] == opening {
            depth += 1;
        } else if bytes[at] == closing {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
        at += 1;
    }
    None
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

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn is_binding_name(value: &str) -> bool {
    value != "_"
        && !matches!(
            value,
            "required"
                | "covariant"
                | "final"
                | "var"
                | "const"
                | "late"
                | "this"
                | "super"
                | "void"
                | "dynamic"
        )
}

fn supports_parameters(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
