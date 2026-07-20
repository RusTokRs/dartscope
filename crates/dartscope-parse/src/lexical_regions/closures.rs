use dartscope_core::{DartFileAnalysis, DartLexicalBindingKind};

use super::scan::{
    arrow_expression_end, arrow_parameter_range, contains_receiver_formal,
    contains_top_level_pattern_start, is_binding_name, is_control_header,
    last_top_level_identifier, matching_delimiter, next_non_whitespace, top_level_assignment,
    top_level_segments, trim_range,
};
use super::{
    IdentifierToken, LexicalRegionAnalysis, binding_for_token, innermost_callable_symbol,
    modeled_callable_header,
};

#[derive(Debug, Clone, Copy)]
struct ClosureRegion {
    parameter_start: usize,
    parameter_end: usize,
    region_start: usize,
    region_end: usize,
    scope_start: usize,
    scope_end: usize,
}

pub(super) fn collect_arrow_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut at = 0usize;
    while at + 1 < bytes.len() {
        if &bytes[at..at + 2] != b"=>" {
            at += 1;
            continue;
        }
        let Some((parameter_start, parameter_end, region_start)) =
            arrow_parameter_range(source, at)
        else {
            at += 2;
            continue;
        };
        if modeled_callable_header(analysis, source, region_start, at) {
            at += 2;
            continue;
        }
        let region_end = arrow_expression_end(source, at + 2);
        collect_region(
            source,
            analysis,
            result,
            ClosureRegion {
                parameter_start,
                parameter_end,
                region_start,
                region_end,
                scope_start: at + 2,
                scope_end: region_end,
            },
        );
        at += 2;
    }
}

pub(super) fn collect_block_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut open = 0usize;
    while open < bytes.len() {
        if bytes[open] != b'(' {
            open += 1;
            continue;
        }
        let Some(close) = matching_delimiter(source, open, b'(', b')', bytes.len()) else {
            open += 1;
            continue;
        };
        let Some(body_open) = next_non_whitespace(bytes, close + 1) else {
            break;
        };
        if bytes.get(body_open) != Some(&b'{')
            || is_control_header(source, open)
            || modeled_callable_header(analysis, source, open, body_open)
        {
            open += 1;
            continue;
        }
        let Some(body_close) = matching_delimiter(source, body_open, b'{', b'}', bytes.len())
        else {
            result.deferred_regions.push((open, bytes.len()));
            open += 1;
            continue;
        };
        collect_region(
            source,
            analysis,
            result,
            ClosureRegion {
                parameter_start: open + 1,
                parameter_end: close,
                region_start: open,
                region_end: body_close + 1,
                scope_start: body_open + 1,
                scope_end: body_close,
            },
        );
        open += 1;
    }
}

fn collect_region(
    source: &str,
    analysis: &DartFileAnalysis,
    result: &mut LexicalRegionAnalysis,
    region: ClosureRegion,
) {
    let Some(owner_id) = innermost_callable_symbol(analysis, region.region_start) else {
        result
            .deferred_regions
            .push((region.region_start, region.region_end));
        return;
    };
    let Some(tokens) = parameter_tokens(source, region.parameter_start, region.parameter_end)
    else {
        result
            .deferred_regions
            .push((region.region_start, region.region_end));
        return;
    };
    result
        .suppressed_regions
        .push((region.region_start, region.scope_start));
    for token in tokens {
        if let Some(binding) = binding_for_token(
            token,
            DartLexicalBindingKind::Parameter,
            "closure_parameter",
            region.scope_start,
            region.scope_end,
            &owner_id,
        ) {
            result.bindings.push(binding);
        }
    }
}

fn parameter_tokens(source: &str, start: usize, end: usize) -> Option<Vec<IdentifierToken<'_>>> {
    let mut tokens = Vec::new();
    collect_parameter_tokens(source, start, end, &mut tokens)?;
    Some(tokens)
}

fn collect_parameter_tokens<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    tokens: &mut Vec<IdentifierToken<'source>>,
) -> Option<()> {
    for (segment_start, segment_end) in top_level_segments(source, start, end, b',') {
        let Some((trimmed_start, trimmed_end)) = trim_range(source, segment_start, segment_end)
        else {
            continue;
        };
        let bytes = source.as_bytes();
        if matches!(
            (
                bytes.get(trimmed_start),
                bytes.get(trimmed_end.saturating_sub(1))
            ),
            (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']'))
        ) {
            collect_parameter_tokens(source, trimmed_start + 1, trimmed_end - 1, tokens)?;
            continue;
        }
        let declaration_end =
            top_level_assignment(source, trimmed_start, trimmed_end).unwrap_or(trimmed_end);
        if contains_receiver_formal(source, trimmed_start, declaration_end)
            || contains_top_level_pattern_start(source, trimmed_start, declaration_end)
        {
            return None;
        }
        let name = last_top_level_identifier(source, trimmed_start, declaration_end)?;
        if is_binding_name(name.text) {
            tokens.push(name);
        }
    }
    Some(())
}
