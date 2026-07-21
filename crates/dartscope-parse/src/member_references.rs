use dartscope_core::{
    Confidence, DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartInvocation, DartLexicalBinding,
};

use crate::member_reference_syntax::{
    declaration_is_static, declaration_name_range, looks_like_type_name,
};
use crate::source_lines::span_for_byte_range;

pub(crate) fn collect_method_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
) -> Vec<DartIdentifierReference> {
    let mut references = method_declaration_references(source, masked_source, analysis);
    for invocation in &analysis.invocations {
        let Some(reference) =
            method_invocation_reference(source, masked_source, analysis, bindings, invocation)
        else {
            continue;
        };
        references.push(reference);
    }
    references.sort_by(|left, right| {
        (
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
    references.dedup();
    references
}

fn method_declaration_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DartDeclarationKind::Method)
        .filter_map(|declaration| {
            let owner_symbol_id = declaration.parent_symbol_id.clone()?;
            let (name_start, name_end) = declaration_name_range(masked_source, declaration)?;
            let kind = if declaration_is_static(masked_source, declaration, name_start) {
                DartIdentifierReferenceKind::MemberDeclarationStatic
            } else {
                DartIdentifierReferenceKind::MemberDeclarationInstance
            };
            Some(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: declaration.name.clone(),
                prefix: Some(owner_symbol_id),
                kind,
                confidence: Confidence::High,
                enclosing_symbol_id: None,
                span: span_for_byte_range(source, name_start, name_end),
            })
        })
        .collect()
}

fn method_invocation_reference(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    invocation: &DartInvocation,
) -> Option<DartIdentifierReference> {
    if has_constructor_keyword(masked_source, invocation.span.byte_start) {
        return None;
    }
    let segments = invocation.target.split('.').collect::<Vec<_>>();
    let (kind, owner, member, confidence) = match segments.as_slice() {
        ["this", member] => (
            DartIdentifierReferenceKind::MemberInvocationInstance,
            enclosing_owner_symbol_id(analysis, invocation)?.to_string(),
            *member,
            Confidence::High,
        ),
        [owner, member]
            if looks_like_type_name(owner)
                && !binding_is_visible(bindings, owner, invocation.span.byte_start) =>
        {
            (
                DartIdentifierReferenceKind::MemberInvocationStatic,
                (*owner).to_string(),
                *member,
                Confidence::Medium,
            )
        }
        [import_prefix, owner, member]
            if looks_like_type_name(owner)
                && analysis
                    .imports
                    .iter()
                    .any(|import| import.prefix.as_deref() == Some(*import_prefix)) =>
        {
            (
                DartIdentifierReferenceKind::MemberInvocationStatic,
                format!("{import_prefix}.{owner}"),
                *member,
                Confidence::High,
            )
        }
        _ => return None,
    };
    let (member_start, member_end) = invocation_member_range(masked_source, invocation, member)?;
    Some(DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: member.to_string(),
        prefix: Some(owner),
        kind,
        confidence,
        enclosing_symbol_id: invocation.enclosing_symbol_id.clone(),
        span: span_for_byte_range(source, member_start, member_end),
    })
}

fn enclosing_owner_symbol_id<'a>(
    analysis: &'a DartFileAnalysis,
    invocation: &DartInvocation,
) -> Option<&'a str> {
    let callable_id = invocation.enclosing_symbol_id.as_deref()?;
    let callable = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.symbol_id.as_deref() == Some(callable_id))?;
    let owner_id = callable.parent_symbol_id.as_deref()?;
    analysis.declarations.iter().find(|declaration| {
        declaration.symbol_id.as_deref() == Some(owner_id) && is_member_owner_kind(declaration.kind)
    })?;
    Some(owner_id)
}

fn invocation_member_range(
    masked_source: &str,
    invocation: &DartInvocation,
    member: &str,
) -> Option<(usize, usize)> {
    let start = invocation.span.byte_start;
    let end = invocation.span.byte_end.min(masked_source.len());
    let expression = masked_source.get(start..end)?;
    let header_end = expression.find('(').unwrap_or(expression.len());
    let header = expression.get(..header_end)?;
    let relative = header.rfind(member)?;
    let member_start = start + relative;
    let member_end = member_start + member.len();
    let bytes = masked_source.as_bytes();
    if member_start > 0
        && bytes
            .get(member_start - 1)
            .is_some_and(|byte| is_identifier_continue(*byte))
    {
        return None;
    }
    if bytes
        .get(member_end)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        return None;
    }
    Some((member_start, member_end))
}

fn has_constructor_keyword(source: &str, start: usize) -> bool {
    let before = source.get(..start).unwrap_or_default().trim_end();
    let token = before
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .next()
        .unwrap_or_default();
    matches!(token, "new" | "const")
}

fn binding_is_visible(bindings: &[DartLexicalBinding], name: &str, at: usize) -> bool {
    bindings.iter().any(|binding| {
        binding.name == name
            && binding.scope_span.byte_start <= at
            && at < binding.scope_span.byte_end
    })
}

fn is_member_owner_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
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

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
