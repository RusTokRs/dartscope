use dartscope_core::{DartPartOfKind, DartStringConstant, SourceSpan};

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

pub(crate) fn extension_declaration_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("extension ")?.trim_start();
    if rest.starts_with("on ") || rest.starts_with("type ") {
        return None;
    }
    next_identifier(rest)
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

pub(crate) fn top_level_function(trimmed: &str, indent: usize) -> Option<String> {
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

pub(crate) fn top_level_variable(trimmed: &str, indent: usize) -> Option<String> {
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

pub(crate) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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
