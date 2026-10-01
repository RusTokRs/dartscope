mod typed;
mod typed_positions;

use std::collections::HashMap;

use dartscope_core::{
    Confidence, DartDeclaration, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartLexicalBinding, SourceSpan,
};

use self::typed::collect_typed_identifier_references;
use self::typed_positions::collect_declaration_type_references;
use crate::binding_index::BindingIndex;
use crate::file_facts::FileFacts;
use crate::identifiers::{is_identifier_continue, is_identifier_start};
use crate::source_lines::span_for_byte_range;
use crate::source_structure::SourceStructure;
use crate::unqualified_member_references;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

pub(crate) fn collect_identifier_references<'a>(
    source: &str,
    masked_source: &str,
    analysis: &'a DartFileAnalysis,
    facts: &FileFacts<'a>,
    bindings: &[DartLexicalBinding],
) -> Vec<DartIdentifierReference> {
    let index = BindingIndex::new(masked_source, &facts.structure, bindings);
    // The parameter names of each callable, read from its header once.
    let mut parameter_names: HashMap<&str, Vec<String>> = HashMap::new();
    let mut references = Vec::new();
    for invocation in &analysis.invocations {
        let Some(root) = identifier_at(masked_source, invocation.span.byte_start) else {
            continue;
        };
        if matches!(root.text, "this" | "super") {
            continue;
        }
        let enclosing_symbol_id = invocation.enclosing_symbol_id.as_deref();
        if lexical_root_is_shadowed(
            masked_source,
            facts,
            &index,
            &mut parameter_names,
            enclosing_symbol_id,
            root,
        ) {
            continue;
        }
        if invocation.target == root.text
            && let Some(member) = unqualified_member_references::enclosing_member(
                &facts.tables,
                masked_source,
                root.text,
                root.start,
            )
            && member.owns_callable()
            && !unqualified_member_references::local_function_shadows(
                facts,
                masked_source,
                &member,
                root.text,
            )
        {
            references.push(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: root.text.to_string(),
                prefix: Some(member.owner_symbol_id.to_string()),
                kind: if member.is_static {
                    DartIdentifierReferenceKind::MemberInvocationStatic
                } else {
                    DartIdentifierReferenceKind::MemberInvocationInstance
                },
                confidence: Confidence::High,
                enclosing_symbol_id: invocation.enclosing_symbol_id.clone(),
                span: span_for_byte_range(source, root.start, root.end),
            });
            continue;
        }
        if owner_member_shadows_invocation(facts, enclosing_symbol_id, root) {
            continue;
        }

        let (name, prefix, confidence, token) = if facts.is_import_prefix(root.text) {
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

    references.extend(collect_typed_identifier_references(
        source,
        masked_source,
        analysis,
    ));
    references.extend(collect_declaration_type_references(
        source,
        masked_source,
        analysis,
        &facts.tables,
    ));
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

fn lexical_root_is_shadowed<'a>(
    masked_source: &str,
    facts: &FileFacts<'a>,
    index: &BindingIndex<'_>,
    parameter_names: &mut HashMap<&'a str, Vec<String>>,
    enclosing_symbol_id: Option<&'a str>,
    root: IdentifierToken<'_>,
) -> bool {
    let Some(owner_id) = enclosing_symbol_id else {
        return false;
    };
    let Some(owner) = facts.tables.by_symbol_id(owner_id) else {
        return false;
    };

    if index
        .owned_by(owner_id, root.text)
        .any(|binding| binding_shadows_invocation(&facts.structure, binding, root))
    {
        return true;
    }

    if parameter_names
        .entry(owner_id)
        .or_insert_with(|| callable_parameter_names(masked_source, owner))
        .iter()
        .any(|name| name == root.text)
    {
        return true;
    }

    facts
        .tables
        .locals_named(owner_id, root.text)
        .any(|declaration| {
            declaration.declaration_span.as_ref().is_some_and(|span| {
                span.byte_start < root.start
                    && local_scope_contains(
                        masked_source,
                        &facts.structure,
                        span.byte_start,
                        root.start,
                        owner,
                    )
            })
        })
}

fn owner_member_shadows_invocation(
    facts: &FileFacts<'_>,
    enclosing_symbol_id: Option<&str>,
    root: IdentifierToken<'_>,
) -> bool {
    let Some(owner_id) = enclosing_symbol_id else {
        return false;
    };
    let Some(owner) = facts.tables.by_symbol_id(owner_id) else {
        return false;
    };
    let Some(type_id) = owner.parent_symbol_id.as_deref() else {
        return false;
    };
    facts.tables.declares_instance_member(type_id, root.text)
}

/// Whether a binding of the invocation's owner and of the spelling's name hides the invocation:
/// the binding is in scope there, or is declared later in the same statement.
fn binding_shadows_invocation(
    structure: &SourceStructure,
    binding: &DartLexicalBinding,
    root: IdentifierToken<'_>,
) -> bool {
    if binding.scope_span.byte_start <= root.start && root.start < binding.scope_span.byte_end {
        return true;
    }
    root.start < binding.scope_span.byte_start
        && structure
            .has_no_statement_boundary_between(root.start, binding.declaration_span.byte_start)
}

fn callable_parameter_names(masked_source: &str, owner: &DartDeclaration) -> Vec<String> {
    let Some(span) = owner.declaration_span.as_ref() else {
        return Vec::new();
    };
    let Some((start, end)) = first_parenthesized_range(masked_source, span) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    collect_parameter_names(&masked_source[start..end], &mut names);
    names
}

fn first_parenthesized_range(source: &str, span: &SourceSpan) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut open = span.byte_start;
    while open < span.byte_end.min(bytes.len()) && bytes[open] != b'(' {
        open += 1;
    }
    if bytes.get(open) != Some(&b'(') {
        return None;
    }

    let mut depth = 1usize;
    let mut at = open + 1;
    while at < span.byte_end.min(bytes.len()) {
        match bytes[at] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open + 1, at));
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn collect_parameter_names(source: &str, names: &mut Vec<String>) {
    for segment in parameter_segments(source) {
        let trimmed = segment.trim();
        if trimmed.len() >= 2
            && matches!(trimmed.as_bytes().first(), Some(&b'{') | Some(&b'['))
            && matches!(trimmed.as_bytes().last(), Some(&b'}') | Some(&b']'))
        {
            collect_parameter_names(&trimmed[1..trimmed.len() - 1], names);
            continue;
        }
        let declaration = trimmed.split('=').next().unwrap_or_default();
        let Some(name) = last_identifier(declaration) else {
            continue;
        };
        if name != "_" && !is_parameter_modifier(name) {
            names.push(name.to_string());
        }
    }
}

fn parameter_segments(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut default_value = false;

    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if !default_value => angles += 1,
            b'>' if !default_value => angles = angles.saturating_sub(1),
            b'=' if parens == 0 && brackets == 0 && braces == 0 => default_value = true,
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                segments.push(&source[start..index]);
                start = index + 1;
                default_value = false;
            }
            _ => {}
        }
    }
    segments.push(&source[start..]);
    segments
}

fn last_identifier(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut at = 0usize;
    let mut last = None;
    while at < bytes.len() {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let end = identifier_end(bytes, at);
        last = Some(&source[at..end]);
        at = end;
    }
    last
}

fn is_parameter_modifier(value: &str) -> bool {
    matches!(
        value,
        "required" | "covariant" | "final" | "var" | "const" | "late" | "this" | "super"
    )
}

/// Whether a local variable declared at `declaration_start` is in scope at `reference_start`: the
/// reference lies before the end of the innermost block of `owner` that was open at the
/// declaration.
fn local_scope_contains(
    masked_source: &str,
    structure: &SourceStructure,
    declaration_start: usize,
    reference_start: usize,
    owner: &DartDeclaration,
) -> bool {
    let Some(owner_span) = owner.declaration_span.as_ref() else {
        return false;
    };
    let limit = declaration_start
        .min(owner_span.byte_end)
        .min(masked_source.len());
    let Some(open) = structure.innermost_open_brace(owner_span.byte_start, limit) else {
        return false;
    };
    structure
        .closing_brace(open)
        .filter(|&close| close < owner_span.byte_end.min(masked_source.len()))
        .is_some_and(|close| reference_start < close)
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
