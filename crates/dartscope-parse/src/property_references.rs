use dartscope_core::{
    Confidence, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartLexicalBinding,
};

use crate::binding_index::BindingIndex;
use crate::file_facts::FileFacts;
use crate::identifiers::{is_identifier_continue, is_identifier_start};
use crate::member_reference_syntax::{
    declaration_is_static, declaration_name_range, looks_like_type_name,
};
use crate::source_lines::span_for_byte_range;

pub(crate) fn collect_property_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
) -> Vec<DartIdentifierReference> {
    let mut references = property_declaration_references(source, masked_source, analysis);
    references.extend(property_access_references(
        source,
        masked_source,
        analysis,
        facts,
        bindings,
    ));
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

fn property_declaration_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| is_property_declaration_kind(declaration.kind))
        .filter_map(|declaration| {
            let owner_symbol_id = declaration.parent_symbol_id.clone()?;
            let (name_start, name_end) = declaration_name_range(masked_source, declaration)?;
            let kind = if declaration_is_static(masked_source, declaration, name_start) {
                DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
            } else {
                DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
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

fn property_access_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    facts: &FileFacts<'_>,
    bindings: &[DartLexicalBinding],
) -> Vec<DartIdentifierReference> {
    let index = BindingIndex::new(masked_source, &facts.structure, bindings);
    let bytes = masked_source.as_bytes();
    let mut references = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let first_start = at;
        let first_end = identifier_end(bytes, first_start);
        at = first_end;
        if is_preceded_by_dot(masked_source, first_start) {
            continue;
        }
        let Some(access) = property_access_at(masked_source, facts, &index, first_start, first_end)
        else {
            continue;
        };
        at = access.member_end;
        if followed_by_call(masked_source, access.member_end) {
            continue;
        }
        let (is_read, is_write) =
            property_access_modes(masked_source, access.expression_start, access.member_end);
        if is_read {
            references.push(property_reference(
                source,
                analysis,
                &access,
                if access.is_static {
                    DartIdentifierReferenceKind::MemberPropertyReadStatic
                } else {
                    DartIdentifierReferenceKind::MemberPropertyReadInstance
                },
            ));
        }
        if is_write {
            references.push(property_reference(
                source,
                analysis,
                &access,
                if access.is_static {
                    DartIdentifierReferenceKind::MemberPropertyWriteStatic
                } else {
                    DartIdentifierReferenceKind::MemberPropertyWriteInstance
                },
            ));
        }
    }
    references
}

#[derive(Debug)]
struct PropertyAccess {
    expression_start: usize,
    member_start: usize,
    member_end: usize,
    member: String,
    owner: String,
    is_static: bool,
    confidence: Confidence,
    enclosing_symbol_id: Option<String>,
}

fn property_access_at(
    masked_source: &str,
    facts: &FileFacts<'_>,
    index: &BindingIndex<'_>,
    first_start: usize,
    first_end: usize,
) -> Option<PropertyAccess> {
    let first = masked_source.get(first_start..first_end)?;
    let (second_start, second_end) = dotted_identifier(masked_source, first_end)?;
    let second = masked_source.get(second_start..second_end)?;
    let third = dotted_identifier(masked_source, second_end);

    if first == "this" {
        if third.is_some() {
            return None;
        }
        let callable = facts.tables.member_callable_at(second_start)?;
        let owner = callable.parent_symbol_id.clone()?;
        return Some(PropertyAccess {
            expression_start: first_start,
            member_start: second_start,
            member_end: second_end,
            member: second.to_string(),
            owner,
            is_static: false,
            confidence: Confidence::High,
            enclosing_symbol_id: callable.symbol_id.clone(),
        });
    }

    if let Some((third_start, third_end)) = third {
        let third_name = masked_source.get(third_start..third_end)?;
        if dotted_identifier(masked_source, third_end).is_some()
            || !looks_like_type_name(second)
            || index.is_visible(first, first_start)
            || !facts.is_import_prefix(first)
        {
            return None;
        }
        return Some(PropertyAccess {
            expression_start: first_start,
            member_start: third_start,
            member_end: third_end,
            member: third_name.to_string(),
            owner: format!("{first}.{second}"),
            is_static: true,
            confidence: Confidence::High,
            enclosing_symbol_id: facts
                .tables
                .member_callable_at(third_start)
                .and_then(|declaration| declaration.symbol_id.clone()),
        });
    }

    if !looks_like_type_name(first)
        || index.is_visible(first, first_start)
        || dotted_identifier(masked_source, second_end).is_some()
    {
        return None;
    }
    Some(PropertyAccess {
        expression_start: first_start,
        member_start: second_start,
        member_end: second_end,
        member: second.to_string(),
        owner: first.to_string(),
        is_static: true,
        confidence: Confidence::Medium,
        enclosing_symbol_id: facts
            .tables
            .member_callable_at(second_start)
            .and_then(|declaration| declaration.symbol_id.clone()),
    })
}

fn property_reference(
    source: &str,
    analysis: &DartFileAnalysis,
    access: &PropertyAccess,
    kind: DartIdentifierReferenceKind,
) -> DartIdentifierReference {
    DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: access.member.clone(),
        prefix: Some(access.owner.clone()),
        kind,
        confidence: access.confidence,
        enclosing_symbol_id: access.enclosing_symbol_id.clone(),
        span: span_for_byte_range(source, access.member_start, access.member_end),
    }
}

fn property_access_modes(source: &str, expression_start: usize, member_end: usize) -> (bool, bool) {
    let before = source
        .get(..expression_start)
        .unwrap_or_default()
        .trim_end();
    if before.ends_with("++") || before.ends_with("--") {
        return (true, true);
    }
    let after = source.get(member_end..).unwrap_or_default().trim_start();
    if after.starts_with("++") || after.starts_with("--") {
        return (true, true);
    }
    if [
        ">>>=", "<<=", ">>=", "??=", "+=", "-=", "*=", "/=", "~/=", "%=", "&=", "|=", "^=",
    ]
    .iter()
    .any(|operator| after.starts_with(operator))
    {
        return (true, true);
    }
    if after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>") {
        return (false, true);
    }
    (true, false)
}

fn dotted_identifier(source: &str, identifier_end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let dot = skip_whitespace(bytes, identifier_end);
    if bytes.get(dot) != Some(&b'.') || bytes.get(dot + 1) == Some(&b'.') {
        return None;
    }
    let start = skip_whitespace(bytes, dot + 1);
    if !bytes
        .get(start)
        .is_some_and(|byte| is_identifier_start(*byte))
    {
        return None;
    }
    Some((start, identifier_end_from(bytes, start)))
}

fn followed_by_call(source: &str, member_end: usize) -> bool {
    let bytes = source.as_bytes();
    let next = skip_whitespace(bytes, member_end);
    bytes.get(next) == Some(&b'(')
}

fn is_preceded_by_dot(source: &str, start: usize) -> bool {
    source
        .get(..start)
        .unwrap_or_default()
        .trim_end()
        .as_bytes()
        .last()
        == Some(&b'.')
}

fn is_property_declaration_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Field | DartDeclarationKind::Getter | DartDeclarationKind::Setter
    )
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn identifier_end(bytes: &[u8], at: usize) -> usize {
    identifier_end_from(bytes, at)
}

fn identifier_end_from(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}
