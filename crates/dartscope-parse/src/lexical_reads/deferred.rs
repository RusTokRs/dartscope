use dartscope_core::{
    DartDeclarationKind, DartFileAnalysis, DartLexicalBinding, DartLexicalBindingKind,
};

pub(super) fn read_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
) -> Vec<(usize, usize)> {
    let mut regions = callable_header_regions(source, analysis, bindings);
    collect_control_regions(source, "for", &mut regions);
    collect_control_regions(source, "catch", &mut regions);
    collect_arrow_closure_regions(source, analysis, &mut regions);
    collect_block_closure_regions(source, analysis, &mut regions);
    regions.sort_unstable();
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

fn collect_control_regions(source: &str, keyword: &str, regions: &mut Vec<(usize, usize)>) {
    let bytes = source.as_bytes();
    let mut search = 0usize;
    while let Some(found) = find_keyword(source, keyword, search) {
        search = found + keyword.len();
        let Some(open) = next_non_whitespace(bytes, search) else {
            continue;
        };
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_delimiter(source, open, b'(', b')', bytes.len()) else {
            continue;
        };
        regions.push((found, following_statement_end(source, close + 1)));
    }
}

fn collect_arrow_closure_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    regions: &mut Vec<(usize, usize)>,
) {
    let bytes = source.as_bytes();
    let mut at = 0usize;
    while at + 1 < bytes.len() {
        if &bytes[at..at + 2] != b"=>" {
            at += 1;
            continue;
        }
        let Some(start) = arrow_parameter_start(source, at) else {
            at += 2;
            continue;
        };
        if !modeled_callable_header(analysis, source, start, at) {
            regions.push((start, arrow_expression_end(source, at + 2)));
        }
        at += 2;
    }
}

fn collect_block_closure_regions(
    source: &str,
    analysis: &DartFileAnalysis,
    regions: &mut Vec<(usize, usize)>,
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
        let Some(brace) = next_non_whitespace(bytes, close + 1) else {
            break;
        };
        if bytes.get(brace) == Some(&b'{')
            && !is_control_header(source, open)
            && !modeled_callable_header(analysis, source, open, brace)
            && let Some(end) = matching_delimiter(source, brace, b'{', b'}', bytes.len())
        {
            regions.push((open, end + 1));
        }
        open += 1;
    }
}

fn modeled_callable_header(
    analysis: &DartFileAnalysis,
    source: &str,
    parameter_start: usize,
    body_start: usize,
) -> bool {
    analysis.declarations.iter().any(|declaration| {
        super::supports_parameters(declaration.kind)
            && declaration.declaration_span.as_ref().is_some_and(|span| {
                span.byte_start <= parameter_start
                    && body_start < span.byte_end
                    && !source[span.byte_start..parameter_start].contains('{')
            })
    })
}

fn arrow_parameter_start(source: &str, arrow: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, arrow)?;
    if bytes[previous] == b')' {
        return matching_open_delimiter(source, previous, b'(', b')');
    }
    let mut start = previous;
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    is_identifier_start(*bytes.get(start)?).then_some(start)
}

fn arrow_expression_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut at = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => break,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => break,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' if braces == 0 => break,
            b'}' => braces -= 1,
            b',' | b';' if parens == 0 && brackets == 0 && braces == 0 => break,
            _ => {}
        }
        at += 1;
    }
    at
}

fn is_control_header(source: &str, open: usize) -> bool {
    let bytes = source.as_bytes();
    let Some(previous) = previous_non_whitespace(bytes, open) else {
        return false;
    };
    let mut start = previous;
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    matches!(
        &source[start..previous + 1],
        "if" | "for" | "while" | "switch" | "catch" | "assert"
    )
}

fn following_statement_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let Some(at) = next_non_whitespace(bytes, start) else {
        return bytes.len();
    };
    if bytes[at] == b'{' {
        return matching_delimiter(source, at, b'{', b'}', bytes.len())
            .map_or(bytes.len(), |close| close + 1);
    }
    source[at..]
        .find(';')
        .map_or(bytes.len(), |relative| at + relative + 1)
}

fn find_keyword(source: &str, keyword: &str, start: usize) -> Option<usize> {
    let mut search = start;
    while let Some(relative) = source[search..].find(keyword) {
        let at = search + relative;
        let before = at
            .checked_sub(1)
            .and_then(|index| source.as_bytes().get(index));
        let after = source.as_bytes().get(at + keyword.len());
        if before.is_none_or(|byte| !is_identifier_continue(*byte))
            && after.is_none_or(|byte| !is_identifier_continue(*byte))
        {
            return Some(at);
        }
        search = at + keyword.len();
    }
    None
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

fn matching_open_delimiter(source: &str, close: usize, opening: u8, closing: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(close) != Some(&closing) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = close;
    while at > 0 {
        at -= 1;
        if bytes[at] == closing {
            depth += 1;
        } else if bytes[at] == opening {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

fn previous_non_whitespace(bytes: &[u8], before: usize) -> Option<usize> {
    let mut at = before;
    while at > 0 {
        at -= 1;
        if !bytes[at].is_ascii_whitespace() {
            return Some(at);
        }
    }
    None
}

fn next_non_whitespace(bytes: &[u8], mut at: usize) -> Option<usize> {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    (at < bytes.len()).then_some(at)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
