use crate::identifiers::{is_identifier_continue, is_identifier_start};

use dartscope_core::{
    Confidence, DartFileAnalysis, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBinding,
};

use crate::binding_index::{BindingIndex, reference_spans};
use crate::file_facts::FileFacts;
use crate::interval_index::IntervalSet;
use crate::lexical_reads::deferred::read_regions;
use crate::lexical_regions::analyze_lexical_regions;
use crate::source_lines::span_for_byte_range;
use crate::unqualified_member_references::{enclosing_member, local_function_shadows};

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
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    let mut references = collect_lexical_target_references(
        source,
        masked_source,
        analysis,
        facts,
        bindings,
        existing_references,
        LexicalTargetMode::SimpleAssignment,
    );
    references.extend(collect_for_in_write_references(
        source,
        masked_source,
        analysis,
        facts,
        bindings,
        existing_references,
    ));
    references
}

fn collect_for_in_write_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    let existing = reference_spans(existing_references);
    let index = BindingIndex::new(masked_source, &facts.structure, bindings);
    analyze_lexical_regions(masked_source, analysis, &facts.tables)
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
                || existing.overlaps(token.start, token.end)
                || index.is_declaration(token.start, token.end)
            {
                return None;
            }
            if index.select_visible(token.text, token.start).is_none() {
                return member_property_references(
                    source,
                    masked_source,
                    analysis,
                    facts,
                    token,
                    MemberTargetMode::Write,
                )
                .into_iter()
                .next();
            }
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
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    collect_lexical_target_references(
        source,
        masked_source,
        analysis,
        facts,
        bindings,
        existing_references,
        LexicalTargetMode::CombinedUpdate,
    )
}

#[derive(Debug, Clone, Copy)]
enum MemberTargetMode {
    Write,
    ReadThenWrite,
}

fn member_target_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    token: IdentifierToken<'_>,
    mode: LexicalTargetMode,
) -> Vec<DartIdentifierReference> {
    let target_mode = match mode {
        LexicalTargetMode::SimpleAssignment => MemberTargetMode::Write,
        LexicalTargetMode::CombinedUpdate => MemberTargetMode::ReadThenWrite,
    };
    member_property_references(source, masked_source, analysis, facts, token, target_mode)
}

fn member_property_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    token: IdentifierToken<'_>,
    mode: MemberTargetMode,
) -> Vec<DartIdentifierReference> {
    let Some(member) = enclosing_member(&facts.tables, masked_source, token.text, token.start)
    else {
        return Vec::new();
    };
    if local_function_shadows(facts, masked_source, &member, token.text) {
        return Vec::new();
    }
    let kinds: &[DartIdentifierReferenceKind] = match mode {
        MemberTargetMode::Write => {
            if !member.owns_writable() {
                return Vec::new();
            }
            &[DartIdentifierReferenceKind::MemberPropertyWriteInstance]
        }
        MemberTargetMode::ReadThenWrite => {
            if !member.owns_named_value() {
                return Vec::new();
            }
            &[
                DartIdentifierReferenceKind::MemberPropertyReadInstance,
                DartIdentifierReferenceKind::MemberPropertyWriteInstance,
            ]
        }
    };
    kinds
        .iter()
        .map(|kind| {
            let kind = if member.is_static {
                static_property_kind(*kind)
            } else {
                *kind
            };
            DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: token.text.to_string(),
                prefix: Some(member.owner_symbol_id.to_string()),
                kind,
                confidence: Confidence::High,
                enclosing_symbol_id: facts
                    .tables
                    .innermost_callable_symbol(token.start)
                    .map(str::to_string),
                span: span_for_byte_range(source, token.start, token.end),
            }
        })
        .collect()
}

fn static_property_kind(kind: DartIdentifierReferenceKind) -> DartIdentifierReferenceKind {
    match kind {
        DartIdentifierReferenceKind::MemberPropertyReadInstance => {
            DartIdentifierReferenceKind::MemberPropertyReadStatic
        }
        DartIdentifierReferenceKind::MemberPropertyWriteInstance => {
            DartIdentifierReferenceKind::MemberPropertyWriteStatic
        }
        _ => kind,
    }
}

fn collect_lexical_target_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
    mode: LexicalTargetMode,
) -> Vec<DartIdentifierReference> {
    let deferred_regions = IntervalSet::new(read_regions(masked_source, analysis, facts, bindings));
    let existing = reference_spans(existing_references);
    let index = BindingIndex::new(masked_source, &facts.structure, bindings);
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

        // The checks are independent of each other; the cheap textual one goes first so that the
        // lookups below only run for tokens that look like an assignment target.
        if token.text == "_"
            || !mode.matches(masked_source, token)
            || deferred_regions.contains(token.start)
            || existing.overlaps(token.start, token.end)
            || index.is_declaration(token.start, token.end)
            || index.is_deferred_local_initializer(token.text, token.start)
        {
            continue;
        }

        let Some(binding) = index.select_visible(token.text, token.start) else {
            references.extend(member_target_references(
                source,
                masked_source,
                analysis,
                facts,
                token,
                mode,
            ));
            continue;
        };
        let enclosing_symbol_id = Some(
            facts
                .tables
                .innermost_callable_symbol(token.start)
                .map_or_else(|| binding.enclosing_symbol_id.clone(), str::to_string),
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
