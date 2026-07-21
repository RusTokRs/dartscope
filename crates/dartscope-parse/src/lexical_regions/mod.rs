mod closures;
mod controls;
mod scan;

use dartscope_core::{DartDeclarationKind, DartFileAnalysis, DartLexicalBindingKind};

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
) -> LexicalRegionAnalysis {
    let mut result = LexicalRegionAnalysis::default();
    controls::collect_for_regions(source, analysis, &mut result);
    controls::collect_catch_regions(source, analysis, &mut result);
    closures::collect_arrow_regions(source, analysis, &mut result);
    closures::collect_block_regions(source, analysis, &mut result);
    result.deferred_regions.sort_unstable();
    result.deferred_regions.dedup();
    let deferred_regions = result.deferred_regions.clone();
    result.bindings.retain(|binding| {
        !deferred_regions.iter().any(|(start, end)| {
            *start <= binding.declaration_start && binding.declaration_start < *end
        })
    });
    result.write_targets.retain(|target| {
        !deferred_regions
            .iter()
            .any(|(start, end)| *start <= target.start && target.start < *end)
    });
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

pub(super) fn innermost_callable_symbol(
    analysis: &DartFileAnalysis,
    offset: usize,
) -> Option<String> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| supports_parameters(declaration.kind))
        .filter_map(|declaration| {
            let span = declaration.declaration_span.as_ref()?;
            (span.byte_start <= offset && offset < span.byte_end).then_some((
                span.byte_end.saturating_sub(span.byte_start),
                declaration.symbol_id.as_ref()?,
            ))
        })
        .min_by_key(|(length, _)| *length)
        .map(|(_, symbol_id)| symbol_id.clone())
}

pub(super) fn modeled_callable_header(
    analysis: &DartFileAnalysis,
    source: &str,
    parameter_start: usize,
    body_start: usize,
) -> bool {
    analysis.declarations.iter().any(|declaration| {
        supports_parameters(declaration.kind)
            && declaration.declaration_span.as_ref().is_some_and(|span| {
                span.byte_start <= parameter_start
                    && body_start < span.byte_end
                    && !source[span.byte_start..parameter_start].contains('{')
                    && !source[span.byte_start..parameter_start].contains("=>")
            })
    })
}

fn supports_parameters(kind: DartDeclarationKind) -> bool {
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
