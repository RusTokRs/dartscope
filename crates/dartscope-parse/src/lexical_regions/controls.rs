use dartscope_core::{DartFileAnalysis, DartLexicalBindingKind};

use super::scan::{
    contains_top_level_pattern_start, find_keyword, find_top_level_keyword,
    following_statement_end, has_top_level_byte, identifier_at, is_binding_name,
    matching_delimiter, next_non_whitespace, top_level_assignment, top_level_byte_positions,
    top_level_identifiers, trim_range,
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
        let Some(body_open) = next_non_whitespace(bytes, close + 1) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        if bytes.get(body_open) != Some(&b'{') {
            result
                .deferred_regions
                .push((found, following_statement_end(source, close + 1)));
            continue;
        }
        let Some(body_close) = matching_delimiter(source, body_open, b'{', b'}', bytes.len())
        else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        let scope_start = body_open + 1;
        let scope_end = body_close;
        let region_end = body_close + 1;
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
    if has_top_level_byte(source, init_start, init_end, b',')
        || contains_top_level_pattern_start(source, init_start, init_end)
    {
        return None;
    }
    let declaration_end = top_level_assignment(source, init_start, init_end).unwrap_or(init_end);
    let tokens = top_level_identifiers(source, init_start, declaration_end);
    if tokens.is_empty() || source[init_start..declaration_end].contains('.') {
        return Some(Vec::new());
    }
    let declares = is_declaration_prefix(tokens[0].text) || tokens.len() >= 2;
    if !declares {
        return Some(Vec::new());
    }
    let name = *tokens.last()?;
    if !is_binding_name(name.text) {
        return None;
    }
    outputs.0.push((init_start, declaration_end));
    binding_for_token(
        name,
        DartLexicalBindingKind::LocalVariable,
        "for_variable",
        semicolons[0] + 1,
        body_end,
        owner_id,
    )
    .map(|binding| vec![binding])
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
            result
                .deferred_regions
                .push((found, following_statement_end(source, close + 1)));
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
