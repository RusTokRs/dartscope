//! Exact-owner member evidence for unqualified member spellings.
//!
//! An unqualified spelling is classified as a member fact only when the enclosing callable supplies
//! one exact owner type and that type directly declares a matching member. Visible lexical
//! bindings, parameters, local variables, and local functions keep their existing shadowing
//! behavior, so a member fact is never fabricated for a spelling they own.

use crate::identifiers::{is_identifier_continue, is_identifier_start};
use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis};

use crate::member_reference_syntax::{
    declaration_is_static, declaration_name_range, declaration_span,
};

/// One unqualified spelling that is provably owned by the exact enclosing type.
pub(crate) struct EnclosingMember<'declaration> {
    pub(crate) owner_symbol_id: &'declaration str,
    pub(crate) declaration: &'declaration DartDeclaration,
    pub(crate) callable: &'declaration DartDeclaration,
    pub(crate) is_static: bool,
}

impl EnclosingMember<'_> {
    pub(crate) fn owns_callable(&self) -> bool {
        matches!(
            self.declaration.kind,
            DartDeclarationKind::Method | DartDeclarationKind::Field | DartDeclarationKind::Getter
        )
    }

    pub(crate) fn owns_readable(&self) -> bool {
        matches!(
            self.declaration.kind,
            DartDeclarationKind::Field | DartDeclarationKind::Getter
        )
    }

    pub(crate) fn owns_writable(&self) -> bool {
        matches!(
            self.declaration.kind,
            DartDeclarationKind::Field | DartDeclarationKind::Setter
        )
    }

    pub(crate) fn owns_named_value(&self) -> bool {
        self.declaration.kind == DartDeclarationKind::Field
    }
}

pub(crate) fn enclosing_member<'analysis>(
    analysis: &'analysis DartFileAnalysis,
    masked_source: &str,
    name: &str,
    at: usize,
) -> Option<EnclosingMember<'analysis>> {
    let callable = enclosing_callable(analysis, at)?;
    let owner = enclosing_owner(analysis, callable)?;
    let declaration = direct_member(analysis, owner, name)?;
    let (name_start, _) = declaration_name_range(masked_source, declaration)?;
    Some(EnclosingMember {
        owner_symbol_id: owner.symbol_id.as_deref()?,
        declaration,
        callable,
        is_static: declaration_is_static(masked_source, declaration, name_start),
    })
}

/// Reports whether a local function declaration shadows the spelling inside the callable body.
///
/// Local functions are deliberately not modeled as declarations yet, so this guard scans the
/// masked callable body for a declaration-shaped occurrence. It only ever suppresses member
/// evidence and never fabricates a fact.
pub(crate) fn local_function_shadows(
    masked_source: &str,
    member: &EnclosingMember<'_>,
    name: &str,
) -> bool {
    let span = declaration_span(member.callable);
    let Some(body_start) = block_body_start(masked_source, span) else {
        return false;
    };
    let end = span.byte_end.min(masked_source.len());
    let bytes = masked_source.as_bytes();
    let mut at = body_start;
    while at < end {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        let token_end = identifier_end(bytes, start);
        at = token_end;
        if masked_source.get(start..token_end) != Some(name)
            || is_preceded_by_dot(masked_source, start)
        {
            continue;
        }
        if local_function_declaration_after(masked_source, token_end, end) {
            return true;
        }
    }
    false
}

fn enclosing_callable(analysis: &DartFileAnalysis, at: usize) -> Option<&DartDeclaration> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| {
            is_callable_kind(declaration.kind) && declaration.parent_symbol_id.is_some()
        })
        .filter(|declaration| {
            let span = declaration_span(declaration);
            span.byte_start <= at && at < span.byte_end
        })
        .min_by_key(|declaration| {
            let span = declaration_span(declaration);
            span.byte_end.saturating_sub(span.byte_start)
        })
}

fn enclosing_owner<'analysis>(
    analysis: &'analysis DartFileAnalysis,
    callable: &DartDeclaration,
) -> Option<&'analysis DartDeclaration> {
    let owner_symbol_id = callable.parent_symbol_id.as_deref()?;
    analysis.declarations.iter().find(|declaration| {
        declaration.symbol_id.as_deref() == Some(owner_symbol_id)
            && is_member_owner_kind(declaration.kind)
    })
}

fn direct_member<'analysis>(
    analysis: &'analysis DartFileAnalysis,
    owner: &DartDeclaration,
    name: &str,
) -> Option<&'analysis DartDeclaration> {
    let owner_symbol_id = owner.symbol_id.as_deref()?;
    analysis.declarations.iter().find(|declaration| {
        declaration.name == name
            && declaration.parent_symbol_id.as_deref() == Some(owner_symbol_id)
            && is_direct_member_kind(declaration.kind)
    })
}

fn block_body_start(source: &str, span: &dartscope_core::SourceSpan) -> Option<usize> {
    let end = span.byte_end.min(source.len());
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut at = span.byte_start;
    while at < end {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' if parens == 0 && brackets == 0 => return Some(at + 1),
            b'=' if parens == 0 && brackets == 0 && bytes.get(at + 1) == Some(&b'>') => {
                return None;
            }
            b';' if parens == 0 && brackets == 0 => return None,
            _ => {}
        }
        at += 1;
    }
    None
}

fn local_function_declaration_after(source: &str, after_name: usize, limit: usize) -> bool {
    let bytes = source.as_bytes();
    let mut at = skip_whitespace(bytes, after_name);
    if bytes.get(at) == Some(&b'<') {
        let Some(after_arguments) = balanced_angle_end(source, at, limit) else {
            return false;
        };
        at = skip_whitespace(bytes, after_arguments);
    }
    if bytes.get(at) != Some(&b'(') {
        return false;
    }
    let Some(close) = matching_paren(source, at, limit) else {
        return false;
    };
    let mut at = skip_whitespace(bytes, close + 1);
    at = skip_function_modifiers(bytes, at);
    at = skip_whitespace(bytes, at);
    bytes.get(at) == Some(&b'{') || bytes.get(at..at + 2).is_some_and(|prefix| prefix == b"=>")
}

fn skip_function_modifiers(bytes: &[u8], mut at: usize) -> usize {
    loop {
        if bytes.get(at) == Some(&b'*') {
            at = skip_whitespace(bytes, at + 1);
            continue;
        }
        let end = identifier_end(bytes, at);
        let token = bytes.get(at..end).unwrap_or_default();
        if token == b"async" || token == b"sync" {
            at = skip_whitespace(bytes, end);
            continue;
        }
        return at;
    }
}

fn balanced_angle_end(source: &str, open: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < limit {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b';' | b'{' | b'}' => return None,
            _ => {}
        }
        at += 1;
    }
    None
}

fn matching_paren(source: &str, open: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < limit {
        match bytes[at] {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
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

fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
            | DartDeclarationKind::Function
    )
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

fn is_direct_member_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Field
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
    )
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
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
