use dartscope_core::{DartPartOfKind, DartStringConstant};

use crate::identifiers::{is_identifier, leading_identifier};
use crate::source_lines::span_for_byte_range;

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

/// Returns the content of the first string literal in `input`.
///
/// Raw strings, triple quotes, escaped quotes, and adjacent literal concatenation are handled through
/// the lexical scanner, so a directive URI is never truncated at an escaped or interior quote.
pub(crate) fn quoted_value(input: &str) -> Option<String> {
    let start = crate::lexical::find_string_literal_start(input, 0)?;
    crate::lexical::string_literals_value(input, start).map(|(value, _)| value)
}

pub(crate) fn class_declaration_name(trimmed: &str) -> Option<String> {
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

pub(crate) fn mixin_declaration_name(trimmed: &str) -> Option<String> {
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

pub(crate) fn extension_type_declaration_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("extension type ")?.trim_start();
    let rest = rest.strip_prefix("const ").unwrap_or(rest);
    next_identifier(rest)
}

/// Returns the declared name of an extension, or an empty name for `extension on T { ... }`.
///
/// An unnamed extension has no declarable name, so the inventory reports it with an empty name rather
/// than dropping the declaration together with every member of its body. `extension type`
/// declarations belong to [`extension_type_declaration_name`].
pub(crate) fn extension_declaration_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.trim_start().strip_prefix("extension")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    match rest.split_whitespace().next()? {
        "type" => None,
        "on" => Some(String::new()),
        name => next_identifier(name),
    }
}

pub(crate) fn name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    next_identifier(rest)
}

pub(crate) fn value_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let marker = format!(" {keyword} ");
    let index = trimmed.find(&marker)?;
    next_qualified_identifier(&trimmed[index + marker.len()..])
}

pub(crate) fn values_after_keyword(trimmed: &str, keyword: &str) -> Vec<String> {
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
    leading_identifier(input).map(str::to_string)
}

fn next_qualified_identifier(input: &str) -> Option<String> {
    let value: String = input
        .chars()
        .take_while(|ch| {
            ch.is_ascii() && (crate::identifiers::is_identifier_continue(*ch as u8) || *ch == '.')
        })
        .collect();
    (!value.is_empty() && value.split('.').all(is_identifier) && !value.ends_with('.'))
        .then_some(value)
}

pub(crate) fn top_level_function(trimmed: &str, indent: usize) -> Option<String> {
    if indent != 0 {
        return None;
    }
    if trimmed.starts_with("get ") || trimmed.starts_with("set ") {
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

pub(crate) fn variable_name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let before_equals = rest.split_once('=').map_or(rest, |(left, _)| left).trim();
    before_equals.split_whitespace().last().map(str::to_string)
}

/// Collects a top-level `const`/`final` string constant whose initializer is a string literal.
///
/// The initializer may span source lines and may be built from adjacent literals; the returned span is
/// the exact literal range rather than the declaration line. An initializer that is not a literal, for
/// example `final value = readString('key');`, is deliberately not reported as a string constant.
pub(crate) fn string_constant_at(
    source: &str,
    line: &str,
    indent: usize,
    byte_start: usize,
) -> Option<DartStringConstant> {
    if indent != 0 {
        return None;
    }
    let (left, _) = line.trim_end_matches(';').split_once('=')?;
    let name = ["const", "final"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(left.trim(), keyword))?;
    let leading = line.len().saturating_sub(line.trim_start().len());
    let literal_start = literal_position(source, byte_start + leading + left.len() + 1)?;
    let (value, end) = crate::lexical::string_literals_value(source, literal_start)?;

    Some(DartStringConstant {
        name,
        value,
        span: span_for_byte_range(source, literal_start, end),
    })
}

/// Returns the byte index of a literal that starts right after the assignment, ignoring whitespace.
fn literal_position(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut at = from.min(bytes.len());
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    crate::lexical::string_literal_range(source, at).map(|_| at)
}

fn is_library_name(value: &str) -> bool {
    value.split('.').all(is_identifier)
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
