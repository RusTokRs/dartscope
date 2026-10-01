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
    // `extension<T> on T { ... }` is an unnamed extension with type parameters.
    if rest.starts_with('<') {
        return skip_angle_group(rest)?
            .trim_start()
            .strip_prefix("on")
            .filter(|tail| tail.starts_with(char::is_whitespace))
            .map(|_| String::new());
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    match rest.split_whitespace().next()? {
        "type" => None,
        "on" => Some(String::new()),
        name => next_identifier(name),
    }
}

/// Returns the text after the `<...>` group that `text` starts with, nested groups included.
fn skip_angle_group(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[index + 1..]);
                }
            }
            b'(' | b')' | b'{' | b'}' | b';' => return None,
            _ => {}
        }
    }
    None
}

pub(crate) fn name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    next_identifier(rest)
}

/// Byte index of the first whole-word `keyword` in `text` that is not inside type arguments or
/// parentheses: preceded and followed by whitespace, so a keyword that starts a continuation line
/// (`\non Widget {`) is found like one in the middle of a line, while the `extends` of a type
/// parameter bound (`class A<T extends B> extends C`) is skipped.
fn keyword_index(text: &str, keyword: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let keyword = keyword.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth = depth.saturating_sub(1),
            _ if depth == 0
                && bytes[index..].starts_with(keyword)
                && index > 0
                && bytes[index - 1].is_ascii_whitespace()
                && bytes
                    .get(index + keyword.len())
                    .is_some_and(u8::is_ascii_whitespace) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn value_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let index = keyword_index(trimmed, keyword)?;
    next_qualified_identifier(trimmed[index + keyword.len()..].trim_start())
}

pub(crate) fn values_after_keyword(trimmed: &str, keyword: &str) -> Vec<String> {
    let Some(index) = keyword_index(trimmed, keyword) else {
        return Vec::new();
    };
    let clause = trimmed[index + keyword.len()..]
        .split(['{', '('])
        .next()
        .unwrap_or_default();
    split_outside_type_arguments(clause)
        .into_iter()
        .filter_map(|part| next_qualified_identifier(part.trim()))
        .collect()
}

/// Splits a clause such as `A<K, V>, B` at the commas that are not inside type arguments.
fn split_outside_type_arguments(clause: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in clause.bytes().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(&clause[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&clause[start..]);
    parts
}

/// The superclass of a mixin application (`class A = B with M;`), which has no `extends` clause.
pub(crate) fn mixin_application_superclass(header: &str) -> Option<String> {
    let (_, after) = header.split_once('=')?;
    next_qualified_identifier(after.trim_start())
}

/// Names of the type parameters an `extension` declares (`extension X<T extends Foo, U> on ...`).
///
/// The list sits between the `extension` keyword (or the extension name) and the `on` keyword.
pub(crate) fn extension_type_parameters(header: &str) -> Vec<String> {
    let Some(rest) = header.trim_start().strip_prefix("extension") else {
        return Vec::new();
    };
    let head = keyword_index(rest, "on").map_or(rest, |index| &rest[..index]);
    let Some(open) = head.find('<') else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut names = Vec::new();
    for (index, byte) in head.bytes().enumerate().skip(open) {
        match byte {
            b'<' => depth += 1,
            b'>' | b',' if depth == 1 => {
                if let Some(name) = leading_identifier(head[start..index].trim()) {
                    names.push(name.to_string());
                }
                start = index + 1;
                if byte == b'>' {
                    break;
                }
            }
            b'>' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    names
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
    callable_name(trimmed)
}

/// Name of the callable whose parameter list opens first in `header`.
///
/// A header can carry parentheses before the declared parameter list: a function type in return
/// position (`void Function(int) make()`) and a record type (`(int, int) pair()`) both come first.
/// Neither is a declaration, so the name is the identifier before the first parameter list that is
/// not preceded by `Function` and not a bare group. A type-parameter list after the name
/// (`first<T>(...)`) is not part of the name.
pub(crate) fn callable_name(header: &str) -> Option<String> {
    callable_name_range(header).map(|(start, end)| header[start..end].to_string())
}

/// Byte range of [`callable_name`] inside `header`.
pub(crate) fn callable_name_range(header: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for (index, byte) in header.bytes().enumerate() {
        match byte {
            b'(' => {
                if depth == 0
                    && let Some((start, end)) = name_range_before_parameters(&header[..index])
                    && &header[start..end] != "Function"
                {
                    return Some((start, end));
                }
                depth += 1;
            }
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Range of the identifier that ends `prefix` once a trailing type-parameter list is removed.
fn name_range_before_parameters(prefix: &str) -> Option<(usize, usize)> {
    let end = strip_type_parameters(prefix.trim_end()).len();
    let stripped = &prefix[..end];
    let start = stripped
        .char_indices()
        .rev()
        .find(|&(_, ch)| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    is_identifier(&stripped[start..]).then_some((start, end))
}

/// Removes a trailing `<...>` (nested angle brackets included) from `text`.
fn strip_type_parameters(text: &str) -> &str {
    if !text.ends_with('>') {
        return text;
    }
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate().rev() {
        match byte {
            b'>' => depth += 1,
            b'<' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return text[..index].trim_end();
                }
            }
            _ => {}
        }
    }
    text
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
