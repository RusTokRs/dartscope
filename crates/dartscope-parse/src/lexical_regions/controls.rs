use dartscope_core::{DartDeclarationKind, DartFileAnalysis, DartLexicalBindingKind};

use super::scan::{
    contains_top_level_pattern_start, find_keyword, find_top_level_keyword, has_top_level_byte,
    identifier_at, is_binding_name, matching_delimiter, next_non_whitespace, top_level_assignment,
    top_level_byte_positions, top_level_identifiers, top_level_segments, trim_range,
};
use super::{LexicalRegionAnalysis, binding_for_token, innermost_callable_symbol, write_for_token};

pub(super) fn collect_for_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut search = 0usize;
    while let Some(found) = find_keyword(source, "for", search) {
        search = found + "for".len();
        let Some(open) = next_non_whitespace(bytes, search) else {
            continue;
        };
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_delimiter(source, open, b'(', b')', bytes.len()) else {
            continue;
        };
        let Some(body_start) = next_non_whitespace(bytes, close + 1) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        let Some((scope_start, scope_end, region_end)) = for_body_region(source, body_start) else {
            result.deferred_regions.push((
                found,
                statement_end(source, body_start).unwrap_or(bytes.len()),
            ));
            continue;
        };
        if contains_local_declaration(analysis, scope_start, scope_end) {
            result.deferred_regions.push((found, region_end));
            continue;
        }
        let Some(owner_id) = innermost_callable_symbol(analysis, found) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        match parse_for_header(
            source,
            open + 1,
            close,
            scope_start,
            scope_end,
            &owner_id,
            (&mut result.suppressed_regions, &mut result.write_targets),
        ) {
            Some(bindings) => result.bindings.extend(bindings),
            None => result.deferred_regions.push((found, region_end)),
        }
    }
}

fn for_body_region(source: &str, body_start: usize) -> Option<(usize, usize, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(body_start) == Some(&b'{') {
        let body_close = matching_delimiter(source, body_start, b'{', b'}', bytes.len())?;
        return Some((body_start + 1, body_close, body_close + 1));
    }
    let body_end = simple_statement_end(source, body_start)?;
    Some((body_start, body_end, body_end))
}

fn simple_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let start = next_non_whitespace(bytes, start)?;
    let token = identifier_at(source, start);
    if bytes.get(start) == Some(&b'{')
        || token.is_some_and(|token| {
            is_control_keyword(token.text) || is_await_for(source, token) || is_label(source, token)
        })
    {
        return None;
    }
    terminated_statement_end(source, start)
}

fn statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let start = next_non_whitespace(bytes, start)?;
    if bytes.get(start) == Some(&b'{') {
        return matching_delimiter(source, start, b'{', b'}', bytes.len()).map(|end| end + 1);
    }
    let Some(token) = identifier_at(source, start) else {
        return terminated_statement_end(source, start);
    };
    if is_label(source, token) {
        let colon = next_non_whitespace(bytes, token.end)?;
        return statement_end(source, colon + 1);
    }
    match token.text {
        "if" => if_statement_end(source, token.end),
        "for" | "while" | "switch" => header_statement_end(source, token.end),
        "await" if is_await_for(source, token) => {
            let for_start = next_non_whitespace(bytes, token.end)?;
            let for_token = identifier_at(source, for_start)?;
            header_statement_end(source, for_token.end)
        }
        "do" => do_statement_end(source, token.end),
        _ => terminated_statement_end(source, start),
    }
}

fn if_statement_end(source: &str, keyword_end: usize) -> Option<usize> {
    let then_end = header_statement_end(source, keyword_end)?;
    let bytes = source.as_bytes();
    let Some(else_start) = next_non_whitespace(bytes, then_end) else {
        return Some(then_end);
    };
    let Some(else_token) = identifier_at(source, else_start) else {
        return Some(then_end);
    };
    if else_token.text != "else" {
        return Some(then_end);
    }
    statement_end(source, else_token.end)
}

fn header_statement_end(source: &str, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let open = next_non_whitespace(bytes, keyword_end)?;
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let close = matching_delimiter(source, open, b'(', b')', bytes.len())?;
    statement_end(source, close + 1)
}

fn do_statement_end(source: &str, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let body_end = statement_end(source, keyword_end)?;
    let while_start = next_non_whitespace(bytes, body_end)?;
    let while_token = identifier_at(source, while_start)?;
    if while_token.text != "while" {
        return None;
    }
    let open = next_non_whitespace(bytes, while_token.end)?;
    let close = matching_delimiter(source, open, b'(', b')', bytes.len())?;
    let semicolon = next_non_whitespace(bytes, close + 1)?;
    (bytes.get(semicolon) == Some(&b';')).then_some(semicolon + 1)
}

fn terminated_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => return None,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => return None,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' if braces == 0 => return None,
            b'}' => braces -= 1,
            b';' if parens == 0 && brackets == 0 && braces == 0 => return Some(at + 1),
            _ => {}
        }
        at += 1;
    }
    None
}

fn is_control_keyword(value: &str) -> bool {
    matches!(value, "if" | "for" | "while" | "do" | "switch" | "try")
}

fn is_await_for(source: &str, token: super::IdentifierToken<'_>) -> bool {
    if token.text != "await" {
        return false;
    }
    let bytes = source.as_bytes();
    next_non_whitespace(bytes, token.end)
        .and_then(|start| identifier_at(source, start))
        .is_some_and(|next| next.text == "for")
}

fn is_label(source: &str, token: super::IdentifierToken<'_>) -> bool {
    next_non_whitespace(source.as_bytes(), token.end)
        .is_some_and(|at| source.as_bytes().get(at) == Some(&b':'))
}

fn contains_local_declaration(
    analysis: &DartFileAnalysis,
    body_start: usize,
    body_end: usize,
) -> bool {
    analysis.declarations.iter().any(|declaration| {
        declaration.kind == DartDeclarationKind::LocalVariable
            && declaration
                .declaration_span
                .as_ref()
                .is_some_and(|span| body_start <= span.byte_start && span.byte_start < body_end)
    })
}

#[derive(Debug, Clone, Copy)]
struct ClassicForDeclarator<'source> {
    token: super::IdentifierToken<'source>,
    declaration_start: usize,
    declaration_end: usize,
    scope_start: usize,
}

#[derive(Debug)]
enum ClassicForInitializer<'source> {
    Expression,
    Declaration(Vec<ClassicForDeclarator<'source>>),
}

fn parse_for_header(
    source: &str,
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    owner_id: &str,
    outputs: (
        &mut Vec<(usize, usize)>,
        &mut Vec<super::LexicalRegionWrite>,
    ),
) -> Option<Vec<super::LexicalRegionBinding>> {
    let semicolons = top_level_byte_positions(source, start, end, b';');
    if semicolons.is_empty() {
        return parse_for_in_header(source, start, end, body_start, body_end, owner_id, outputs);
    }
    if semicolons.len() != 2 {
        return None;
    }
    let Some((init_start, init_end)) = trim_range(source, start, semicolons[0]) else {
        return Some(Vec::new());
    };
    let initializer = parse_classic_for_initializer(source, init_start, init_end)?;
    let ClassicForInitializer::Declaration(declarators) = initializer else {
        return Some(Vec::new());
    };
    let bindings = declarators
        .iter()
        .map(|declarator| {
            binding_for_token(
                declarator.token,
                DartLexicalBindingKind::LocalVariable,
                "for_variable",
                declarator.scope_start,
                body_end,
                owner_id,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    outputs.0.extend(
        declarators
            .iter()
            .map(|declarator| (declarator.declaration_start, declarator.declaration_end)),
    );
    Some(bindings)
}

fn parse_classic_for_initializer<'source>(
    source: &'source str,
    start: usize,
    end: usize,
) -> Option<ClassicForInitializer<'source>> {
    let segments = top_level_segments(source, start, end, b',');
    let (first_start, first_end) = *segments.first()?;
    let (first_start, first_end) = trim_range(source, first_start, first_end)?;
    let declaration_end = top_level_assignment(source, first_start, first_end).unwrap_or(first_end);
    if contains_top_level_pattern_start(source, first_start, declaration_end) {
        return None;
    }
    let tokens = top_level_identifiers(source, first_start, declaration_end);
    let declares = !tokens.is_empty()
        && !source[first_start..declaration_end].contains('.')
        && (is_declaration_prefix(tokens[0].text) || tokens.len() >= 2);
    if !declares {
        return (segments.len() == 1
            && !contains_top_level_pattern_start(source, first_start, first_end))
        .then_some(ClassicForInitializer::Expression);
    }

    let mut declarators = Vec::with_capacity(segments.len());
    declarators.push(parse_classic_for_declarator(
        source,
        first_start,
        first_end,
        true,
    )?);
    for (segment_start, segment_end) in segments.into_iter().skip(1) {
        declarators.push(parse_classic_for_declarator(
            source,
            segment_start,
            segment_end,
            false,
        )?);
    }
    Some(ClassicForInitializer::Declaration(declarators))
}

fn parse_classic_for_declarator<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    first: bool,
) -> Option<ClassicForDeclarator<'source>> {
    let (start, end) = trim_range(source, start, end)?;
    let assignment = top_level_assignment(source, start, end);
    let declaration_end = assignment.unwrap_or(end);
    let (declaration_start, declaration_name_end) = trim_range(source, start, declaration_end)?;
    if contains_top_level_pattern_start(source, declaration_start, declaration_name_end)
        || source[declaration_start..declaration_name_end].contains('.')
    {
        return None;
    }
    let tokens = top_level_identifiers(source, declaration_start, declaration_name_end);
    let token = *tokens.last()?;
    if !is_binding_name(token.text) {
        return None;
    }
    if !first
        && (tokens.len() != 1
            || token.start != declaration_start
            || token.end != declaration_name_end)
    {
        return None;
    }
    let scope_start = match assignment {
        Some(assignment) => trim_range(source, assignment + 1, end)?.1,
        None => token.end,
    };
    Some(ClassicForDeclarator {
        token,
        declaration_start: start,
        declaration_end,
        scope_start,
    })
}

fn parse_for_in_header(
    source: &str,
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    owner_id: &str,
    outputs: (
        &mut Vec<(usize, usize)>,
        &mut Vec<super::LexicalRegionWrite>,
    ),
) -> Option<Vec<super::LexicalRegionBinding>> {
    let in_at = find_top_level_keyword(source, start, end, "in")?;
    let (left_start, left_end) = trim_range(source, start, in_at)?;
    if has_top_level_byte(source, left_start, left_end, b',')
        || contains_top_level_pattern_start(source, left_start, left_end)
        || source[left_start..left_end].contains('.')
    {
        return None;
    }
    let tokens = top_level_identifiers(source, left_start, left_end);
    if tokens.is_empty() {
        return None;
    }
    let declares = is_declaration_prefix(tokens[0].text) || tokens.len() >= 2;
    if !declares {
        let target = *tokens.first()?;
        if tokens.len() != 1
            || target.start != left_start
            || target.end != left_end
            || !is_binding_name(target.text)
        {
            return None;
        }
        outputs.0.push((left_start, left_end));
        outputs.1.push(write_for_token(target, owner_id)?);
        return Some(Vec::new());
    }
    let name = *tokens.last()?;
    if !is_binding_name(name.text) {
        return None;
    }
    outputs.0.push((left_start, left_end));
    binding_for_token(
        name,
        DartLexicalBindingKind::LocalVariable,
        "for_variable",
        body_start,
        body_end,
        owner_id,
    )
    .map(|binding| vec![binding])
}

pub(super) fn collect_catch_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut search = 0usize;
    while let Some(found) = find_keyword(source, "catch", search) {
        search = found + "catch".len();
        let Some(open) = next_non_whitespace(bytes, search) else {
            continue;
        };
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_delimiter(source, open, b'(', b')', bytes.len()) else {
            continue;
        };
        let Some(body_open) = next_non_whitespace(bytes, close + 1) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        if bytes.get(body_open) != Some(&b'{') {
            result.deferred_regions.push((
                found,
                statement_end(source, body_open).unwrap_or(bytes.len()),
            ));
            continue;
        }
        let Some(body_close) = matching_delimiter(source, body_open, b'{', b'}', bytes.len())
        else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        let region_end = body_close + 1;
        let Some(owner_id) = innermost_callable_symbol(analysis, found) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        let Some(tokens) = simple_identifier_segments(source, open + 1, close, 2) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        result.suppressed_regions.push((open + 1, close));
        for token in tokens {
            if let Some(binding) = binding_for_token(
                token,
                DartLexicalBindingKind::LocalVariable,
                "catch_parameter",
                body_open + 1,
                body_close,
                &owner_id,
            ) {
                result.bindings.push(binding);
            }
        }
    }
}

fn simple_identifier_segments(
    source: &str,
    start: usize,
    end: usize,
    max_segments: usize,
) -> Option<Vec<super::IdentifierToken<'_>>> {
    let segments = super::scan::top_level_segments(source, start, end, b',');
    if segments.is_empty() || segments.len() > max_segments {
        return None;
    }
    let mut tokens = Vec::new();
    for (segment_start, segment_end) in segments {
        let (segment_start, segment_end) = trim_range(source, segment_start, segment_end)?;
        let token = identifier_at(source, segment_start)?;
        if token.end != segment_end {
            return None;
        }
        if is_binding_name(token.text) {
            tokens.push(token);
        }
    }
    Some(tokens)
}

fn is_declaration_prefix(value: &str) -> bool {
    matches!(value, "var" | "final" | "const" | "late")
}
