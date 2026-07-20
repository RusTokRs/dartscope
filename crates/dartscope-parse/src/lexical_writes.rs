use std::cmp::Reverse;

use dartscope_core::{
    Confidence, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartLexicalBinding, DartLexicalBindingKind,
};

use crate::lexical_reads::deferred::read_regions;
use crate::lexical_regions::analyze_lexical_regions;
use crate::source_lines::span_for_byte_range;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy)]
enum LexicalTargetMode {
    SimpleAssignment,
    CombinedUpdate,
}

pub(crate) fn collect_lexical_write_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    let mut references = collect_lexical_target_references(
        source,
        masked_source,
        analysis,
        bindings,
        existing_references,
        LexicalTargetMode::SimpleAssignment,
    );
    references.extend(collect_for_in_write_references(
        source,
        masked_source,
        analysis,
        bindings,
        existing_references,
    ));
    references
}

fn collect_for_in_write_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    analyze_lexical_regions(masked_source, analysis)
        .write_targets
        .into_iter()
        .filter_map(|target| {
            let text = masked_source.get(target.start..target.end)?;
            let token = IdentifierToken {
                text,
                start: target.start,
                end: target.end,
            };
            if token.text != target.name
                || overlaps_existing_reference(existing_references, token)
                || is_binding_declaration(bindings, token)
            {
                return None;
            }
            select_visible_binding(bindings, token)?;
            Some(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: target.name,
                prefix: None,
                kind: DartIdentifierReferenceKind::VariableWrite,
                confidence: Confidence::High,
                enclosing_symbol_id: Some(target.owner_id),
                span: span_for_byte_range(source, target.start, target.end),
            })
        })
        .collect()
}

pub(crate) fn collect_lexical_update_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    collect_lexical_target_references(
        source,
        masked_source,
        analysis,
        bindings,
        existing_references,
        LexicalTargetMode::CombinedUpdate,
    )
}

fn collect_lexical_target_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
    mode: LexicalTargetMode,
) -> Vec<DartIdentifierReference> {
    let deferred_regions = read_regions(masked_source, analysis, bindings);
    let bytes = masked_source.as_bytes();
    let mut references = Vec::new();
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
            || !mode.matches(masked_source, token)
        {
            continue;
        }

        let Some(binding) = select_visible_binding(bindings, token) else {
            continue;
        };
        let enclosing_symbol_id = Some(
            innermost_callable_symbol(analysis, token.start)
                .unwrap_or_else(|| binding.enclosing_symbol_id.clone()),
        );
        let span = span_for_byte_range(source, token.start, token.end);
        for kind in mode.reference_kinds() {
            references.push(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: token.text.to_string(),
                prefix: None,
                kind: *kind,
                confidence: Confidence::High,
                enclosing_symbol_id: enclosing_symbol_id.clone(),
                span: span.clone(),
            });
        }
    }

    references
}

impl LexicalTargetMode {
    fn matches(self, source: &str, token: IdentifierToken<'_>) -> bool {
        match self {
            Self::SimpleAssignment => is_simple_assignment_target(source, token),
            Self::CombinedUpdate => is_combined_update_target(source, token),
        }
    }

    fn reference_kinds(self) -> &'static [DartIdentifierReferenceKind] {
        const SIMPLE_ASSIGNMENT: [DartIdentifierReferenceKind; 1] =
            [DartIdentifierReferenceKind::VariableWrite];
        const COMBINED_UPDATE: [DartIdentifierReferenceKind; 2] = [
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ];

        match self {
            Self::SimpleAssignment => &SIMPLE_ASSIGNMENT,
            Self::CombinedUpdate => &COMBINED_UPDATE,
        }
    }
}

fn is_simple_assignment_target(source: &str, token: IdentifierToken<'_>) -> bool {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, token.start);
    let Some(next) = next_non_whitespace(bytes, token.end) else {
        return false;
    };

    is_unqualified_target(bytes, previous, Some(next))
        && bytes[next..].starts_with(b"=")
        && !bytes[next..].starts_with(b"==")
        && !bytes[next..].starts_with(b"=>")
}

fn is_combined_update_target(source: &str, token: IdentifierToken<'_>) -> bool {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, token.start);
    let next = next_non_whitespace(bytes, token.end);

    is_unqualified_target(bytes, previous, next)
        && (next.is_some_and(|at| {
            compound_assignment_operator_at(bytes, at) || starts_increment_operator(bytes, at)
        }) || ends_increment_operator(bytes, previous))
}

fn is_unqualified_target(bytes: &[u8], previous: Option<usize>, next: Option<usize>) -> bool {
    !previous.is_some_and(|at| matches!(bytes[at], b'.' | b'@'))
        && next.is_none_or(|at| !matches!(bytes[at], b'.' | b'['))
}

fn compound_assignment_operator_at(bytes: &[u8], at: usize) -> bool {
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
}

fn starts_increment_operator(bytes: &[u8], at: usize) -> bool {
    bytes[at..].starts_with(b"++") || bytes[at..].starts_with(b"--")
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
