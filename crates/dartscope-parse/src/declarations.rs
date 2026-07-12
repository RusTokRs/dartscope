use std::collections::HashMap;

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartPartOfKind, DartStringConstant, SourceSpan,
};

use crate::source_lines::source_lines;

pub(crate) fn library_directive_name(trimmed: &str) -> Option<Option<String>> {
    let rest = trimmed.strip_prefix("library")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) && !rest.starts_with(';') {
        return None;
    }
    let name = rest.trim().trim_end_matches(';').trim();
    if name.is_empty() {
        Some(None)
    } else {
        is_library_name(name).then(|| Some(name.to_string()))
    }
}

pub(crate) fn part_of_value(trimmed: &str) -> Option<(String, DartPartOfKind)> {
    let rest = trimmed.strip_prefix("part of")?.trim();
    quoted_value(rest)
        .map(|uri| (uri, DartPartOfKind::Uri))
        .or_else(|| {
            rest.trim_end_matches(';')
                .split_whitespace()
                .next()
                .filter(|name| is_library_name(name))
                .map(|name| (name.to_string(), DartPartOfKind::LibraryName))
        })
}

pub(crate) fn quoted_value(input: &str) -> Option<String> {
    let quote = input.find(['\'', '"'])?;
    let quote_char = input.as_bytes()[quote] as char;
    let rest = &input[quote + 1..];
    let end = rest.find(quote_char)?;
    Some(rest[..end].to_string())
}

pub(crate) fn declaration_from_line(
    trimmed: &str,
    indent: usize,
    span: SourceSpan,
) -> Option<DartDeclaration> {
    if let Some(name) = class_declaration_name(trimmed) {
        return Some(DartDeclaration {
            name,
            kind: DartDeclarationKind::Class,
            span,
            extends: value_after_keyword(trimmed, "extends"),
            mixes_in: values_after_keyword(trimmed, "with"),
        });
    }
    if let Some(name) = mixin_declaration_name(trimmed) {
        return Some(simple_declaration(name, DartDeclarationKind::Mixin, span));
    }
    if let Some(name) = name_after_keyword(trimmed, "enum") {
        return Some(simple_declaration(name, DartDeclarationKind::Enum, span));
    }
    if let Some(name) = extension_type_declaration_name(trimmed) {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::ExtensionType,
            span,
        ));
    }
    if let Some(name) = extension_declaration_name(trimmed) {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::Extension,
            span,
        ));
    }
    if let Some(name) = name_after_keyword(trimmed, "typedef") {
        return Some(simple_declaration(name, DartDeclarationKind::Typedef, span));
    }
    if let Some(name) = top_level_variable(trimmed, indent) {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::Variable,
            span,
        ));
    }
    top_level_function(trimmed, indent)
        .map(|name| simple_declaration(name, DartDeclarationKind::Function, span))
}

fn class_declaration_name(trimmed: &str) -> Option<String> {
    let tokens: Vec<_> = trimmed.split_whitespace().collect();
    let class_index = tokens.iter().position(|token| *token == "class")?;
    if !tokens[..class_index].iter().all(|token| {
        matches!(
            *token,
            "abstract" | "base" | "final" | "interface" | "sealed" | "mixin"
        )
    }) {
        return None;
    }
    tokens
        .get(class_index + 1)
        .and_then(|token| next_identifier(token))
}

fn mixin_declaration_name(trimmed: &str) -> Option<String> {
    let tokens: Vec<_> = trimmed.split_whitespace().collect();
    let mixin_index = tokens.iter().position(|token| *token == "mixin")?;
    if !tokens[..mixin_index].iter().all(|token| *token == "base")
        || tokens.get(mixin_index + 1) == Some(&"class")
    {
        return None;
    }
    tokens
        .get(mixin_index + 1)
        .and_then(|token| next_identifier(token))
}

fn extension_type_declaration_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("extension type ")?.trim_start();
    let rest = rest.strip_prefix("const ").unwrap_or(rest);
    next_identifier(rest)
}

fn extension_declaration_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("extension ")?.trim_start();
    if rest.starts_with("on ") || rest.starts_with("type ") {
        return None;
    }
    next_identifier(rest)
}

fn simple_declaration(
    name: String,
    kind: DartDeclarationKind,
    span: SourceSpan,
) -> DartDeclaration {
    DartDeclaration {
        name,
        kind,
        span,
        extends: None,
        mixes_in: Vec::new(),
    }
}

fn name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    next_identifier(rest)
}

fn value_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let marker = format!(" {keyword} ");
    let index = trimmed.find(&marker)?;
    next_qualified_identifier(&trimmed[index + marker.len()..])
}

fn values_after_keyword(trimmed: &str, keyword: &str) -> Vec<String> {
    let marker = format!(" {keyword} ");
    let Some(index) = trimmed.find(&marker) else {
        return Vec::new();
    };
    trimmed[index + marker.len()..]
        .split(['{', '('])
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| next_qualified_identifier(part.trim()))
        .collect()
}

pub(crate) fn next_identifier(input: &str) -> Option<String> {
    let ident: String = input
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    (!ident.is_empty()).then_some(ident)
}

fn next_qualified_identifier(input: &str) -> Option<String> {
    let value: String = input
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(*ch, '_' | '.'))
        .collect();
    (!value.is_empty() && value.split('.').all(is_identifier) && !value.ends_with('.'))
        .then_some(value)
}

fn top_level_function(trimmed: &str, indent: usize) -> Option<String> {
    if indent != 0 {
        return None;
    }
    if !trimmed.ends_with('{') && !trimmed.ends_with("=>") && !trimmed.contains('(') {
        return None;
    }
    if trimmed
        .split_once('(')
        .is_some_and(|(before_paren, _)| before_paren.contains('='))
    {
        return None;
    }
    if trimmed.starts_with("if ") || trimmed.starts_with("for ") || trimmed.starts_with("while ") {
        return None;
    }
    let before_paren = trimmed.split_once('(')?.0.trim();
    let name = before_paren.split_whitespace().last()?;
    is_identifier(name).then_some(name.to_string())
}

fn top_level_variable(trimmed: &str, indent: usize) -> Option<String> {
    if indent != 0 {
        return None;
    }
    ["const", "final", "var"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(trimmed, keyword))
}

pub(crate) fn variable_name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let before_equals = rest.split_once('=').map_or(rest, |(left, _)| left).trim();
    before_equals.split_whitespace().last().map(str::to_string)
}

pub(crate) fn string_constant_from_line(
    trimmed: &str,
    indent: usize,
    span: SourceSpan,
) -> Option<DartStringConstant> {
    if indent != 0 {
        return None;
    }
    let (left, right) = trimmed.trim_end_matches(';').split_once('=')?;
    let left = left.trim();
    let right = right.trim();
    let name = ["const", "final"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(left, keyword))?;
    let value = quoted_value(right)?;

    Some(DartStringConstant { name, value, span })
}

pub(crate) fn collect_string_constant_values(
    source: &str,
    masked_source: &str,
) -> HashMap<String, String> {
    source_lines(source)
        .into_iter()
        .zip(source_lines(masked_source))
        .filter_map(|(source_line, masked_line)| {
            let indent = masked_line
                .text
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .count();
            let masked_trimmed = masked_line.text.trim();
            top_level_variable(masked_trimmed, indent)?;
            string_constant_from_line(
                source_line.text.trim(),
                indent,
                SourceSpan::line(source_line.number, source_line.byte_start, source_line.text),
            )
        })
        .map(|constant| (constant.name, constant.value))
        .collect()
}

pub(crate) fn resolve_interpolated_string(
    value: &str,
    string_constants: &HashMap<String, String>,
) -> Option<String> {
    if !value.contains('$') {
        return Some(value.to_string());
    }

    let mut resolved = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            resolved.push(ch);
            continue;
        }

        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }

        if name.is_empty() {
            return None;
        }
        resolved.push_str(string_constants.get(&name)?);
    }

    Some(resolved)
}

pub(crate) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_library_name(value: &str) -> bool {
    value.split('.').all(is_identifier)
}

pub(crate) fn is_flutter_base(base: &str) -> bool {
    let base = base.rsplit('.').next().unwrap_or(base);
    matches!(
        base,
        "Widget"
            | "StatelessWidget"
            | "StatefulWidget"
            | "InheritedWidget"
            | "State"
            | "ConsumerWidget"
    )
}

pub(crate) fn is_flutter_import(uri: &str) -> bool {
    uri.starts_with("package:flutter/") || uri.starts_with("package:flutter_riverpod/")
}

pub(crate) fn directive_like_without_semicolon(trimmed: &str) -> bool {
    (starts_keyword(trimmed, "part") || starts_keyword(trimmed, "part of"))
        && !trimmed.ends_with(';')
}

fn starts_keyword(line: &str, keyword: &str) -> bool {
    line == keyword
        || line
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}
