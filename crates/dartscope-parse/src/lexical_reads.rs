use crate::identifiers::{is_identifier_continue, is_identifier_start};

use dartscope_core::{
    Confidence, DartFileAnalysis, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBinding,
};

use crate::binding_index::{BindingIndex, reference_spans};
use crate::file_facts::FileFacts;
use crate::interval_index::IntervalSet;
use crate::source_lines::span_for_byte_range;
use crate::source_structure::SourceStructure;
use crate::unqualified_member_references::{enclosing_member, local_function_shadows};

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
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
    existing_references: &[DartIdentifierReference],
) -> Vec<DartIdentifierReference> {
    let deferred_regions = IntervalSet::new(deferred::read_regions(
        masked_source,
        analysis,
        facts,
        bindings,
    ));
    let existing = reference_spans(existing_references);
    let index = BindingIndex::new(masked_source, &facts.structure, bindings);
    let bytes = masked_source.as_bytes();
    let assignments = assignment_positions(bytes);
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
            || deferred_regions.contains(token.start)
            || existing.overlaps(token.start, token.end)
            || index.is_declaration(token.start, token.end)
            || index.is_deferred_local_initializer(token.text, token.start)
            || index.is_local_declaration_prefix(token.start)
            || !is_conservative_read_position(masked_source, &facts.structure, &assignments, token)
        {
            continue;
        }

        let Some(binding) = index.select_visible(token.text, token.start) else {
            if let Some(reference) =
                member_read_reference(source, masked_source, analysis, facts, token)
            {
                reads.push(reference);
            }
            continue;
        };
        reads.push(DartIdentifierReference {
            source_path: analysis.path.clone(),
            name: token.text.to_string(),
            prefix: None,
            kind: DartIdentifierReferenceKind::VariableRead,
            confidence: Confidence::High,
            enclosing_symbol_id: Some(
                facts
                    .tables
                    .innermost_callable_symbol(token.start)
                    .map_or_else(|| binding.enclosing_symbol_id.clone(), str::to_string),
            ),
            span: span_for_byte_range(source, token.start, token.end),
        });
    }

    reads
}

fn member_read_reference(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    token: IdentifierToken<'_>,
) -> Option<DartIdentifierReference> {
    let member = enclosing_member(&facts.tables, masked_source, token.text, token.start)?;
    if !member.owns_readable() || local_function_shadows(facts, masked_source, &member, token.text)
    {
        return None;
    }
    Some(DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: token.text.to_string(),
        prefix: Some(member.owner_symbol_id.to_string()),
        kind: if member.is_static {
            DartIdentifierReferenceKind::MemberPropertyReadStatic
        } else {
            DartIdentifierReferenceKind::MemberPropertyReadInstance
        },
        confidence: Confidence::High,
        enclosing_symbol_id: facts
            .tables
            .innermost_callable_symbol(token.start)
            .map(str::to_string),
        span: span_for_byte_range(source, token.start, token.end),
    })
}

fn is_conservative_read_position(
    source: &str,
    structure: &SourceStructure,
    assignments: &[usize],
    token: IdentifierToken<'_>,
) -> bool {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, token.start);
    let next = next_non_whitespace(bytes, token.end);

    !previous.is_some_and(|at| matches!(bytes[at], b'.' | b'@'))
        && next.is_none_or(|at| bytes[at] != b':')
        && !starts_write_operator(bytes, next)
        && !ends_increment_operator(bytes, previous)
        && !precedes_assignment_in_statement(assignments, structure, token.end)
        && !follows_type_keyword(source, token.start)
        && !structure.is_inside_angle_pair(token.start)
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

/// Whether an assignment operator follows `start` before the expression that contains it ends.
///
/// Such a token is part of an assignment target (`a.b = c`, `list[i] = x`) and is not a plain read.
/// `assignments` holds the position of every assignment operator of the text, and
/// `SourceStructure::expression_end` the place where the expression stops, so the answer needs no
/// scan; it is exactly what scanning forward from `start` for an operator, stopping at the first
/// `;`, `,` or `{` outside nested groups (or the `}` of an enclosing block), finds.
fn precedes_assignment_in_statement(
    assignments: &[usize],
    structure: &SourceStructure,
    start: usize,
) -> bool {
    let first = assignments.partition_point(|&at| at < start);
    assignments.get(first).is_some_and(|&at| {
        structure
            .expression_end(start)
            .is_none_or(|end| at < end)
    })
}

/// The position of every assignment operator in `bytes`, in increasing order.
fn assignment_positions(bytes: &[u8]) -> Vec<usize> {
    (0..bytes.len())
        .filter(|&at| {
            matches!(
                bytes[at],
                b'>' | b'<' | b'?' | b'~' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'='
            ) && assignment_operator_at(bytes, at)
        })
        .collect()
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

#[cfg(test)]
mod tests;
