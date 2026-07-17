use dartscope_core::{
    Confidence, DartFileAnalysis, DartIdentifierReference, DartIdentifierReferenceKind,
};

use crate::source_lines::span_for_byte_range;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

pub(crate) fn collect_identifier_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    let mut references = Vec::new();
    for invocation in &analysis.invocations {
        let Some(root) = identifier_at(masked_source, invocation.span.byte_start) else {
            continue;
        };
        if matches!(root.text, "this" | "super") {
            continue;
        }

        let import_prefix = analysis
            .imports
            .iter()
            .any(|import| import.prefix.as_deref() == Some(root.text));
        let (name, prefix, confidence, token) = if import_prefix {
            let Some(member) = next_dotted_identifier(masked_source, root.end) else {
                continue;
            };
            (
                member.text.to_string(),
                Some(root.text.to_string()),
                Confidence::High,
                member,
            )
        } else {
            (root.text.to_string(), None, Confidence::Medium, root)
        };

        references.push(DartIdentifierReference {
            source_path: analysis.path.clone(),
            name,
            prefix,
            kind: DartIdentifierReferenceKind::InvocationTarget,
            confidence,
            enclosing_symbol_id: invocation.enclosing_symbol_id.clone(),
            span: span_for_byte_range(source, token.start, token.end),
        });
    }

    sort_identifier_references(&mut references);
    references.dedup_by(|left, right| {
        left.source_path == right.source_path
            && left.name == right.name
            && left.prefix == right.prefix
            && left.kind == right.kind
            && left.span.byte_start == right.span.byte_start
            && left.span.byte_end == right.span.byte_end
    });
    references
}

pub(crate) fn sort_identifier_references(references: &mut [DartIdentifierReference]) {
    references.sort_by(|left, right| {
        (
            &left.source_path,
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                &right.source_path,
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
}

fn identifier_at(source: &str, start: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    if !bytes
        .get(start)
        .is_some_and(|byte| is_identifier_start(*byte))
    {
        return None;
    }
    let end = identifier_end(bytes, start);
    Some(IdentifierToken {
        text: &source[start..end],
        start,
        end,
    })
}

fn next_dotted_identifier(source: &str, after: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    let dot = skip_whitespace(bytes, after);
    if bytes.get(dot) != Some(&b'.') {
        return None;
    }
    let start = skip_whitespace(bytes, dot + 1);
    identifier_at(source, start)
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

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
