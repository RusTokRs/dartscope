use std::cmp::Reverse;

use dartscope_core::{
    Confidence, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartLexicalBinding, DartLexicalBindingKind,
};

use crate::source_lines::span_for_byte_range;

pub(crate) mod deferred;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

pub(crate) fn collect_lexical_read_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    let deferred_regions = deferred::read_regions(masked_source, analysis, bindings);
    let bytes = masked_source.as_bytes();
    let mut reads = Vec::new();
    let mut at = 0usize;

    while at < bytes.len() {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let end = identifier_end(bytes, at);
        let token = IdentifierToken {
            text: &masked_source[at..end],
            start: at,
            end,
        };
        at = end;

        if token.text == "_"
            || deferred_regions
                .iter()
                .any(|(start, end)| *start <= token.start && token.start < *end)
            || overlaps_existing_reference(existing_references, token)
            || is_binding_declaration(bindings, token)
            || is_deferred_local_initializer(masked_source, bindings, token)
            || is_local_declaration_prefix(masked_source, bindings, token)
            || !is_conservative_read_position(masked_source, token)
        {
            continue;
        }

        let Some(binding) = select_visible_binding(bindings, token) else {
            continue;
        };
        reads.push(DartIdentifierReference {
            source_path: analysis.path.clone(),
            name: token.text.to_string(),
            prefix: None,
            kind: DartIdentifierReferenceKind::VariableRead,
            confidence: Confidence::High,
            enclosing_symbol_id: Some(
                innermost_callable_symbol(analysis, token.start)
                    .unwrap_or_else(|| binding.enclosing_symbol_id.clone()),
            ),
            span: span_for_byte_range(source, token.start, token.end),
        });
    }

    reads
}

fn select_visible_binding<'a>(
    bindings: &'a [DartLexicalBinding],
    token: IdentifierToken<'_>,
) -> Option<&'a DartLexicalBinding> {
    let mut best = None;
    let mut best_rank = None;
    let mut ambiguous = false;

    for binding in bindings.iter().filter(|binding| {
        binding.name == token.text
            && binding.scope_span.byte_start <= token.start
            && token.start < binding.scope_span.byte_end
    }) {
        let rank = binding_rank(binding);
        match best_rank {
            None => {
                best = Some(binding);
                best_rank = Some(rank);
                ambiguous = false;
            }
            Some(current) if rank < current => {
                best = Some(binding);
                best_rank = Some(rank);
                ambiguous = false;
            }
            Some(current) if rank == current => ambiguous = true,
            Some(_) => {}
        }
    }

    if ambiguous { None } else { best }
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

fn overlaps_existing_reference(
    references: &[DartIdentifierReference],
    token: IdentifierToken<'_>,
) -> bool {
    references.iter().any(|reference| {
        reference.span.byte_start < token.end && token.start < reference.span.byte_end
    })
}

fn is_binding_declaration(bindings: &[DartLexicalBinding], token: IdentifierToken<'_>) -> bool {
    bindings.iter().any(|binding| {
        binding.declaration_span.byte_start <= token.start
            && token.end <= binding.declaration_span.byte_end
    })
}

fn is_deferred_local_initializer(
    source: &str,
    bindings: &[DartLexicalBinding],
    token: IdentifierToken<'_>,
) -> bool {
    bindings.iter().any(|binding| {
        binding.kind == DartLexicalBindingKind::LocalVariable
            && binding.name == token.text
            && statement_start(source, binding.declaration_span.byte_start) <= token.start
            && token.start < binding.scope_span.byte_start
    })
}

fn is_local_declaration_prefix(
    source: &str,
    bindings: &[DartLexicalBinding],
    token: IdentifierToken<'_>,
) -> bool {
    bindings.iter().any(|binding| {
        if binding.kind != DartLexicalBindingKind::LocalVariable
            || token.start >= binding.declaration_span.byte_start
        {
            return false;
        }
        let statement_start = statement_start(source, binding.declaration_span.byte_start);
        let segment_start = declarator_segment_start(
            source,
            statement_start,
            binding.declaration_span.byte_start,
        );
        segment_start <= token.start
    })
}

fn declarator_segment_start(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = end.min(bytes.len());
    while at > start {
        at -= 1;
        match bytes[at] {
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                return at + 1;
            }
            b')' => parens += 1,
            b'(' => parens = parens.saturating_sub(1),
            b']' => brackets += 1,
            b'[' => brackets = brackets.saturating_sub(1),
            b'}' => braces += 1,
            b'{' => braces = braces.saturating_sub(1),
            b'>' => angles += 1,
            b'<' => angles = angles.saturating_sub(1),
            _ => {}
        }
    }
    start
}

fn is_conservative_read_position(source: &str, token: IdentifierToken<'_>) -> bool {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, token.start);
    let next = next_non_whitespace(bytes, token.end);

    !previous.is_some_and(|at| matches!(bytes[at], b'.' | b'@'))
        && next.is_none_or(|at| bytes[at] != b':')
        && !starts_write_operator(bytes, next)
        && !ends_increment_operator(bytes, previous)
        && !precedes_assignment_in_statement(source, token.end)
        && !follows_type_keyword(source, token.start)
        && !is_inside_angle_pair(source, token.start)
}

fn follows_type_keyword(source: &str, before: usize) -> bool {
    previous_identifier(source, before).is_some_and(|identifier| {
        matches!(
            identifier,
            "as" | "is"
                | "new"
                | "const"
                | "extends"
                | "implements"
                | "with"
                | "on"
                | "class"
                | "mixin"
                | "enum"
                | "extension"
                | "typedef"
        )
    })
}

fn previous_identifier(source: &str, before: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let end = previous_non_whitespace(bytes, before)? + 1;
    if !is_identifier_continue(*bytes.get(end - 1)?) {
        return None;
    }
    let mut start = end - 1;
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    source.get(start..end)
}

fn is_inside_angle_pair(source: &str, offset: usize) -> bool {
    let bytes = source.as_bytes();
    let start = statement_start(source, offset);
    let mut depth = 0usize;
    for byte in &bytes[start..offset.min(bytes.len())] {
        match byte {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if depth == 0 {
        return false;
    }

    for byte in &bytes[offset.min(bytes.len())..] {
        match byte {
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return true;
                }
            }
            b';' | b'{' | b'}' if depth > 0 => return false,
            _ => {}
        }
    }
    false
}

fn starts_write_operator(bytes: &[u8], at: Option<usize>) -> bool {
    let Some(at) = at else {
        return false;
    };
    assignment_operator_at(bytes, at)
        || bytes[at..].starts_with(b"++")
        || bytes[at..].starts_with(b"--")
}

fn ends_increment_operator(bytes: &[u8], at: Option<usize>) -> bool {
    let Some(at) = at else {
        return false;
    };
    at > 0
        && bytes
            .get(at - 1..=at)
            .is_some_and(|operator| operator == b"++" || operator == b"--")
}

fn precedes_assignment_in_statement(source: &str, start: usize) -> bool {
    let bytes = source.as_bytes();
    let mut at = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;

    while at < bytes.len() {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' if parens == 0 && brackets == 0 && braces == 0 => break,
            b'{' => braces += 1,
            b'}' if braces == 0 => break,
            b'}' => braces -= 1,
            b',' | b';' if parens == 0 && brackets == 0 && braces == 0 => break,
            _ => {}
        }
        if assignment_operator_at(bytes, at) {
            return true;
        }
        at += 1;
    }
    false
}

fn assignment_operator_at(bytes: &[u8], at: usize) -> bool {
    let tail = &bytes[at..];
    [
        b">>>=".as_slice(),
        b"<<=".as_slice(),
        b">>=".as_slice(),
        b"??=".as_slice(),
        b"~/=".as_slice(),
        b"+=".as_slice(),
        b"-=".as_slice(),
        b"*=".as_slice(),
        b"/=".as_slice(),
        b"%=".as_slice(),
        b"&=".as_slice(),
        b"|=".as_slice(),
        b"^=".as_slice(),
    ]
    .iter()
    .any(|operator| tail.starts_with(operator))
        || (tail.starts_with(b"=")
            && !tail.starts_with(b"==")
            && !tail.starts_with(b"=>")
            && at
                .checked_sub(1)
                .and_then(|index| bytes.get(index))
                .is_none_or(|byte| !matches!(*byte, b'!' | b'<' | b'>')))
}

pub(super) fn supports_parameters(kind: DartDeclarationKind) -> bool {
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

fn innermost_callable_symbol(analysis: &DartFileAnalysis, offset: usize) -> Option<String> {
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

fn statement_start(source: &str, before: usize) -> usize {
    let bytes = source.as_bytes();
    let mut at = before.min(bytes.len());
    while at > 0 {
        at -= 1;
        if matches!(bytes[at], b';' | b'{' | b'}') {
            return at + 1;
        }
    }
    0
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
