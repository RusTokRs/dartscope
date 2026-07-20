use dartscope_core::{
    DartDeclarationKind, DartFileAnalysis, DartLexicalBinding, DartLexicalBindingKind,
};

use crate::lexical_regions::analyze_lexical_regions;

pub(crate) fn read_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
) -> Vec<(usize, usize)> {
    let mut regions = callable_header_regions(source, analysis, bindings);
    let lexical_regions = analyze_lexical_regions(source, analysis);
    regions.extend(lexical_regions.deferred_regions);
    regions.extend(lexical_regions.suppressed_regions);
    regions.sort_unstable();
    regions.dedup();
    regions
}

fn callable_header_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
) -> Vec<(usize, usize)> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| super::supports_parameters(declaration.kind))
        .filter_map(|declaration| {
            let span = declaration.declaration_span.as_ref()?;
            let owner = declaration.symbol_id.as_deref();
            let parameter_scope_start = owner.and_then(|owner| {
                bindings
                    .iter()
                    .filter(|binding| {
                        binding.kind == DartLexicalBindingKind::Parameter
                            && binding.enclosing_symbol_id == owner
                            && !binding.symbol_id.contains("/closure_parameter:")
                    })
                    .map(|binding| binding.scope_span.byte_start)
                    .min()
            });
            let end = parameter_scope_start.or_else(|| {
                callable_header_end(source, span.byte_start, span.byte_end, declaration.kind)
            })?;
            (span.byte_start < end).then_some((span.byte_start, end))
        })
        .collect()
}

fn callable_header_end(
    source: &str,
    start: usize,
    end: usize,
    kind: DartDeclarationKind,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if kind == DartDeclarationKind::Getter {
        return (start..end.min(bytes.len())).find(|at| {
            bytes[*at] == b'{'
                || bytes[*at] == b';'
                || bytes
                    .get(*at..*at + 2)
                    .is_some_and(|operator| operator == b"=>")
        });
    }
    let open = (start..end.min(bytes.len())).find(|at| bytes[*at] == b'(')?;
    matching_delimiter(source, open, b'(', b')', end).map(|close| close + 1)
}

fn matching_delimiter(
    source: &str,
    open: usize,
    opening: u8,
    closing: u8,
    limit: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&opening) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = open + 1;
    while at < limit.min(bytes.len()) {
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
