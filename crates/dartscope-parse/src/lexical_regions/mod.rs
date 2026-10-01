mod closures;
mod controls;
mod scan;

use std::cmp::Reverse;

use dartscope_core::{DartFileAnalysis, DartLexicalBindingKind};

use crate::declaration_tables::{DeclarationTables, supports_parameters};
use crate::interval_index::{IntervalSet, StabbingIndex};
use crate::lexical::mask_non_code;

#[derive(Debug, Clone)]
pub(crate) struct LexicalRegionBinding {
    pub(crate) name: String,
    pub(crate) kind: DartLexicalBindingKind,
    pub(crate) symbol_segment: &'static str,
    pub(crate) declaration_start: usize,
    pub(crate) declaration_end: usize,
    pub(crate) scope_start: usize,
    pub(crate) scope_end: usize,
    pub(crate) owner_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LexicalRegionWrite {
    pub(crate) name: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) owner_id: String,
}

#[derive(Debug, Default)]
pub(crate) struct LexicalRegionAnalysis {
    pub(crate) bindings: Vec<LexicalRegionBinding>,
    pub(crate) write_targets: Vec<LexicalRegionWrite>,
    pub(crate) deferred_regions: Vec<(usize, usize)>,
    pub(crate) suppressed_regions: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct IdentifierToken<'source> {
    pub(super) text: &'source str,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(crate) fn analyze_lexical_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    tables: &DeclarationTables<'_>,
) -> LexicalRegionAnalysis {
    let masked_source = mask_non_code(source).code;
    let headers = CallableHeaders::new(analysis, source);
    let mut result = LexicalRegionAnalysis::default();
    controls::collect_for_regions(&masked_source, tables, &mut result);
    controls::collect_catch_regions(&masked_source, tables, &mut result);
    closures::collect_arrow_regions(source, tables, &headers, &mut result);
    closures::collect_block_regions(source, tables, &headers, &mut result);
    result.deferred_regions.sort_unstable();
    result.deferred_regions.dedup();
    let deferred_regions = IntervalSet::new(result.deferred_regions.iter().copied());
    result
        .bindings
        .retain(|binding| !deferred_regions.contains(binding.declaration_start));
    result
        .write_targets
        .retain(|target| !deferred_regions.contains(target.start));
    result.bindings.sort_by(|left, right| {
        (
            left.declaration_start,
            left.declaration_end,
            left.kind,
            &left.name,
            left.scope_start,
            left.scope_end,
        )
            .cmp(&(
                right.declaration_start,
                right.declaration_end,
                right.kind,
                &right.name,
                right.scope_start,
                right.scope_end,
            ))
    });
    result.write_targets.sort_by(|left, right| {
        (left.start, left.end, &left.name, &left.owner_id).cmp(&(
            right.start,
            right.end,
            &right.name,
            &right.owner_id,
        ))
    });
    result.write_targets.dedup_by(|left, right| {
        left.start == right.start
            && left.end == right.end
            && left.name == right.name
            && left.owner_id == right.owner_id
    });
    result.suppressed_regions.sort_unstable();
    result.suppressed_regions.dedup();
    result
}

pub(super) fn write_for_token(
    token: IdentifierToken<'_>,
    owner_id: &str,
) -> Option<LexicalRegionWrite> {
    if token.text == "_" {
        return None;
    }
    Some(LexicalRegionWrite {
        name: token.text.to_string(),
        start: token.start,
        end: token.end,
        owner_id: owner_id.to_string(),
    })
}

pub(super) fn binding_for_token(
    token: IdentifierToken<'_>,
    kind: DartLexicalBindingKind,
    symbol_segment: &'static str,
    scope_start: usize,
    scope_end: usize,
    owner_id: &str,
) -> Option<LexicalRegionBinding> {
    if token.text == "_" || scope_start > scope_end {
        return None;
    }
    Some(LexicalRegionBinding {
        name: token.text.to_string(),
        kind,
        symbol_segment,
        declaration_start: token.start,
        declaration_end: token.end,
        scope_start,
        scope_end,
        owner_id: owner_id.to_string(),
    })
}

/// Tells whether a parenthesis that opens a closure-shaped region is really the parameter list of
/// a declaration the file analysis already models.
pub(super) struct CallableHeaders {
    /// For a position `p`, the callable with the furthest end among those whose header (the text
    /// from the start of the declaration to the first `{` or `=>`) reaches `p`.
    index: StabbingIndex<Reverse<usize>>,
}

impl CallableHeaders {
    fn new(analysis: &DartFileAnalysis, source: &str) -> Self {
        let bytes = source.as_bytes();
        let items = analysis
            .declarations
            .iter()
            .enumerate()
            .filter(|(_, declaration)| supports_parameters(declaration.kind))
            .filter_map(|(id, declaration)| {
                let span = declaration.declaration_span.as_ref()?;
                let window = bytes.get(span.byte_start..span.byte_end.min(bytes.len()))?;
                // A header that holds `{` or `=>` before the parenthesis is not a header of this
                // callable: the parenthesis belongs to something inside its body.
                let brace = window.iter().position(|&byte| byte == b'{');
                let arrow = window.windows(2).position(|pair| pair == b"=>");
                let last_position = [
                    brace.map(|offset| span.byte_start + offset),
                    arrow.map(|offset| span.byte_start + offset + 1),
                ]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(usize::MAX);
                Some((
                    span.byte_start,
                    last_position.saturating_add(1),
                    Reverse(span.byte_end),
                    id,
                ))
            })
            .collect();
        Self {
            index: StabbingIndex::new(items),
        }
    }

    /// Whether some callable starts at or before `parameter_start`, ends after `body_start` and
    /// has neither `{` nor `=>` between its start and `parameter_start`.
    pub(super) fn models(&self, parameter_start: usize, body_start: usize) -> bool {
        self.index
            .best_at(parameter_start)
            .is_some_and(|stab| body_start < stab.key.0)
    }
}
