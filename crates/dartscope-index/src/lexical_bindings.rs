use std::cmp::Reverse;

use dartscope_core::{
    DartLexicalBinding, DartLexicalBindingQuery, DartLexicalBindingResolution,
    DartLexicalBindingResolutionStatus, DartProjectReferenceAnalysis, normalize_path,
};

/// Selects the most specific parser-produced lexical binding visible at one byte offset.
pub fn resolve_project_lexical_binding(
    analysis: &DartProjectReferenceAnalysis,
    mut query: DartLexicalBindingQuery,
) -> DartLexicalBindingResolution {
    query.source_path = normalize_path(query.source_path);
    if !analysis
        .project
        .files
        .iter()
        .any(|file| file.path == query.source_path)
    {
        return DartLexicalBindingResolution {
            query,
            status: DartLexicalBindingResolutionStatus::SourceFileMissing,
            candidates: Vec::new(),
        };
    }

    let mut visible: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| binding.source_path == query.source_path)
        .filter(|binding| binding.name == query.name)
        .filter(|binding| {
            query
                .enclosing_symbol_id
                .as_deref()
                .is_none_or(|owner| binding.enclosing_symbol_id == owner)
        })
        .filter(|binding| {
            binding.scope_span.byte_start <= query.byte_offset
                && query.byte_offset < binding.scope_span.byte_end
        })
        .cloned()
        .collect();

    visible.sort_by_key(binding_rank);
    let Some(best) = visible.first().map(binding_rank) else {
        return DartLexicalBindingResolution {
            query,
            status: DartLexicalBindingResolutionStatus::Missing,
            candidates: Vec::new(),
        };
    };
    visible.retain(|binding| binding_rank(binding) == best);
    let status = if visible.len() == 1 {
        DartLexicalBindingResolutionStatus::Resolved
    } else {
        DartLexicalBindingResolutionStatus::Ambiguous
    };
    DartLexicalBindingResolution {
        query,
        status,
        candidates: visible,
    }
}

fn binding_rank(binding: &DartLexicalBinding) -> (usize, Reverse<usize>, usize, usize) {
    (
        binding
            .scope_span
            .byte_end
            .saturating_sub(binding.scope_span.byte_start),
        Reverse(binding.declaration_span.byte_start),
        binding.scope_span.byte_start,
        binding.scope_span.byte_end,
    )
}
