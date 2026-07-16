use std::collections::HashMap;

use dartscope_core::DartDeclarationKind;

use super::scanner::EndMode;
use crate::declarations::{
    class_declaration_name, extension_declaration_name, extension_type_declaration_name,
    is_identifier, mixin_declaration_name, name_after_keyword,
};

pub(super) fn type_header(header: &str) -> Option<(String, DartDeclarationKind)> {
    class_declaration_name(header)
        .map(|name| (name, DartDeclarationKind::Class))
        .or_else(|| mixin_declaration_name(header).map(|name| (name, DartDeclarationKind::Mixin)))
        .or_else(|| {
            name_after_keyword(header, "enum").map(|name| (name, DartDeclarationKind::Enum))
        })
        .or_else(|| {
            extension_type_declaration_name(header)
                .map(|name| (name, DartDeclarationKind::ExtensionType))
        })
        .or_else(|| {
            extension_declaration_name(header).map(|name| (name, DartDeclarationKind::Extension))
        })
        .or_else(|| {
            name_after_keyword(header, "typedef").map(|name| (name, DartDeclarationKind::Typedef))
        })
}

pub(super) fn member_headers(
    header: &str,
    owner_name: &str,
) -> Vec<(String, DartDeclarationKind, EndMode)> {
    let cleaned = strip_member_modifiers(header);
    let before_paren = cleaned.split_once('(').map(|(left, _)| left.trim());

    if let Some(before) = before_paren
        && (before == owner_name
            || before
                .strip_prefix(owner_name)
                .is_some_and(|rest| rest.strip_prefix('.').is_some_and(is_identifier)))
    {
        return vec![(
            before.to_string(),
            DartDeclarationKind::Constructor,
            EndMode::BodyOrSemicolon,
        )];
    }

    if let Some(name) = name_after_token(cleaned, "get") {
        return vec![(name, DartDeclarationKind::Getter, EndMode::BodyOrSemicolon)];
    }
    if let Some(name) = name_after_token(cleaned, "set") {
        return vec![(name, DartDeclarationKind::Setter, EndMode::BodyOrSemicolon)];
    }
    if let Some(name) = operator_name(cleaned) {
        return vec![(
            name,
            DartDeclarationKind::Operator,
            EndMode::BodyOrSemicolon,
        )];
    }

    if let Some(before) = before_paren {
        if before.contains('=') || starts_control_keyword(before) {
            return Vec::new();
        }
        let Some(name) = before.split_whitespace().last() else {
            return Vec::new();
        };
        if is_identifier(name) {
            return vec![(
                name.to_string(),
                DartDeclarationKind::Method,
                EndMode::BodyOrSemicolon,
            )];
        }
    }

    field_names(cleaned)
        .into_iter()
        .map(|name| (name, DartDeclarationKind::Field, EndMode::SemicolonOnly))
        .collect()
}

pub(super) fn local_variable_names(header: &str) -> Vec<String> {
    let header = header.strip_prefix("late ").unwrap_or(header);
    if starts_control_keyword(header)
        || ["return ", "throw ", "yield ", "await ", "case "]
            .iter()
            .any(|prefix| header.starts_with(prefix))
    {
        return Vec::new();
    }
    if let Some(without_keyword) = ["var", "final", "const"]
        .into_iter()
        .find_map(|keyword| header.strip_prefix(keyword).map(str::trim_start))
    {
        declared_names(without_keyword, false)
    } else {
        declared_names(header, true)
    }
}

fn field_names(header: &str) -> Vec<String> {
    if header.starts_with("return ") || header.starts_with("throw ") || header.contains("=>") {
        return Vec::new();
    }
    declared_names(header, true)
}

fn declared_names(header: &str, require_type: bool) -> Vec<String> {
    let header = header.trim_end_matches(';').trim();
    if header.is_empty() {
        return Vec::new();
    }

    let segments = split_top_level_commas(header);
    let mut names = Vec::new();
    for (index, segment) in segments.into_iter().enumerate() {
        let left = segment
            .split_once('=')
            .map_or(segment, |(left, _)| left)
            .trim();
        if left.contains('(') {
            continue;
        }
        let Some(candidate) = left.split_whitespace().last() else {
            continue;
        };
        let candidate = candidate.trim_start_matches(['?', '!']);
        if !is_identifier(candidate) {
            continue;
        }
        if index == 0 && require_type && left.split_whitespace().count() < 2 {
            continue;
        }
        names.push(candidate.to_string());
    }
    names
}

fn split_top_level_commas(value: &str) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                parts.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

fn strip_member_modifiers(mut header: &str) -> &str {
    loop {
        let trimmed = header.trim_start();
        let Some((first, rest)) = trimmed.split_once(char::is_whitespace) else {
            return trimmed;
        };
        if matches!(
            first,
            "abstract"
                | "augment"
                | "const"
                | "covariant"
                | "external"
                | "factory"
                | "final"
                | "late"
                | "static"
        ) {
            header = rest;
        } else {
            return trimmed;
        }
    }
}

fn name_after_token(header: &str, token: &str) -> Option<String> {
    let tokens: Vec<_> = header.split_whitespace().collect();
    let index = tokens.iter().position(|item| *item == token)?;
    tokens.get(index + 1).and_then(|item| {
        let name: String = item
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        is_identifier(&name).then_some(name)
    })
}

fn operator_name(header: &str) -> Option<String> {
    let (_, rest) = header.split_once("operator")?;
    let token = rest.split_whitespace().next()?;
    let name: String = token.chars().take_while(|ch| *ch != '(').collect();
    let name = name.as_str();
    matches!(
        name,
        "<" | ">"
            | "<="
            | ">="
            | "=="
            | "~"
            | "-"
            | "+"
            | "/"
            | "~/"
            | "*"
            | "%"
            | "|"
            | "^"
            | "&"
            | "<<"
            | ">>>"
            | ">>"
            | "[]="
            | "[]"
    )
    .then(|| name.to_string())
}

pub(super) fn is_concise_constructor(header: &str) -> bool {
    let header = header.trim_start();
    header.starts_with("new(")
        || header.starts_with("new ")
        || header.starts_with("const new")
        || header.starts_with("factory(")
        || header.starts_with("factory ")
        || header.starts_with("const factory")
}

pub(super) fn has_primary_constructor(header: &str, name: &str) -> bool {
    let Some(index) = header.find(name) else {
        return false;
    };
    header[index + name.len()..].trim_start().starts_with('(')
}

pub(super) fn is_directive(header: &str) -> bool {
    ["import ", "export ", "part ", "part of ", "library "]
        .iter()
        .any(|prefix| header.trim_start().starts_with(prefix))
}

fn starts_control_keyword(value: &str) -> bool {
    [
        "if", "for", "while", "switch", "catch", "return", "throw", "assert",
    ]
    .iter()
    .any(|keyword| value == *keyword || value.starts_with(&format!("{keyword} ")))
}

pub(super) fn is_type_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

pub(super) fn is_callable_kind(kind: DartDeclarationKind) -> bool {
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

pub(super) fn kind_label(kind: DartDeclarationKind) -> &'static str {
    match kind {
        DartDeclarationKind::Class => "class",
        DartDeclarationKind::Mixin => "mixin",
        DartDeclarationKind::Enum => "enum",
        DartDeclarationKind::Extension => "extension",
        DartDeclarationKind::ExtensionType => "extension_type",
        DartDeclarationKind::Typedef => "typedef",
        DartDeclarationKind::Function => "function",
        DartDeclarationKind::Variable => "variable",
        DartDeclarationKind::Method => "method",
        DartDeclarationKind::Constructor => "constructor",
        DartDeclarationKind::Field => "field",
        DartDeclarationKind::Getter => "getter",
        DartDeclarationKind::Setter => "setter",
        DartDeclarationKind::Operator => "operator",
        DartDeclarationKind::LocalVariable => "local_variable",
    }
}

#[derive(Default)]
pub(super) struct SymbolIdAllocator {
    counts: HashMap<String, usize>,
}

impl SymbolIdAllocator {
    pub(super) fn allocate(&mut self, base: String) -> String {
        let count = self.counts.entry(base.clone()).or_default();
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}#{}", *count)
        }
    }
}
