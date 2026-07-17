use dartscope_core::{DartInvocationArgument, DartMapEntry};

use super::scanner::matching_delimiter;
use crate::declarations::is_identifier;
use crate::source_lines::{line_span_for_byte, span_for_byte_range};

pub(super) fn invocation_arguments(
    source: &str,
    masked_source: &str,
    start: usize,
    end: usize,
) -> Vec<DartInvocationArgument> {
    split_top_level(masked_source, start, end, b',')
        .into_iter()
        .filter_map(|(segment_start, segment_end)| {
            invocation_argument(source, masked_source, segment_start, segment_end)
        })
        .collect()
}

fn invocation_argument(
    source: &str,
    masked_source: &str,
    start: usize,
    end: usize,
) -> Option<DartInvocationArgument> {
    let (start, end) = trim_range(source, start, end);
    if start >= end {
        return None;
    }
    let colon = top_level_delimiter(masked_source, start, end, b':');
    let (name, expression_start) = colon
        .and_then(|colon| {
            let name = masked_source[start..colon].trim();
            is_identifier(name).then(|| (Some(name.to_string()), colon + 1))
        })
        .unwrap_or((None, start));
    let (expression_start, expression_end) = trim_range(source, expression_start, end);
    if expression_start >= expression_end {
        return None;
    }
    let expression = source[expression_start..expression_end].to_string();
    Some(DartInvocationArgument {
        name,
        string_value: string_literal_value(&expression),
        map_entries: map_entries(source, masked_source, expression_start, expression_end),
        expression,
        span: span_for_byte_range(source, start, end),
    })
}

fn map_entries(source: &str, masked_source: &str, start: usize, end: usize) -> Vec<DartMapEntry> {
    let Some(open) = first_top_level_byte(masked_source, start, end, b'{') else {
        return Vec::new();
    };
    let Some(close) = matching_delimiter(masked_source, open, b'{', b'}') else {
        return Vec::new();
    };
    if close >= end {
        return Vec::new();
    }
    split_top_level(masked_source, open + 1, close, b',')
        .into_iter()
        .filter_map(|(entry_start, entry_end)| {
            let (entry_start, entry_end) = trim_range(source, entry_start, entry_end);
            let entry_start = trim_leading_trivia(source, entry_start, entry_end);
            let colon = top_level_delimiter(masked_source, entry_start, entry_end, b':')?;
            let (key_start, key_end) = trim_range(source, entry_start, colon);
            let (value_start, value_end) = trim_range(source, colon + 1, entry_end);
            (key_start < key_end && value_start < value_end).then(|| {
                let key = source[key_start..key_end].to_string();
                DartMapEntry {
                    string_key: string_literal_value(&key),
                    key,
                    value: source[value_start..value_end].to_string(),
                    span: span_for_byte_range(source, entry_start, entry_end),
                    source_line_span: line_span_for_byte(source, entry_start),
                }
            })
        })
        .collect()
}

fn split_top_level(source: &str, start: usize, end: usize, delimiter: u8) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut part_start = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut index = start;
    while index < end.min(bytes.len()) {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            byte if byte == delimiter
                && parens == 0
                && brackets == 0
                && braces == 0
                && angles == 0 =>
            {
                parts.push((part_start, index));
                part_start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push((part_start, end));
    parts
}

fn top_level_delimiter(source: &str, start: usize, end: usize, delimiter: u8) -> Option<usize> {
    split_top_level(source, start, end, delimiter)
        .first()
        .and_then(|(_, first_end)| (*first_end < end).then_some(*first_end))
}

fn first_top_level_byte(source: &str, start: usize, end: usize, needle: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    for (index, byte) in bytes
        .iter()
        .copied()
        .enumerate()
        .take(end.min(bytes.len()))
        .skip(start)
    {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            byte if byte == needle && parens == 0 && brackets == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn trim_leading_trivia(source: &str, mut start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while start < end && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        if source[start..end].starts_with("//") {
            start = source[start..end]
                .find(['\n', '\r'])
                .map_or(end, |offset| start + offset + 1);
            continue;
        }
        if source[start..end].starts_with("/*") {
            start = source[start + 2..end]
                .find("*/")
                .map_or(end, |offset| start + 2 + offset + 2);
            continue;
        }
        return start;
    }
}

fn trim_range(source: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (start, end)
}

pub(super) fn string_literal_value(expression: &str) -> Option<String> {
    let expression = expression.trim();
    let (raw, expression) = expression
        .strip_prefix('r')
        .map_or((false, expression), |rest| (true, rest));
    let quote = *expression.as_bytes().first()?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let triple = expression.as_bytes().starts_with(&[quote, quote, quote]);
    let width = if triple { 3 } else { 1 };
    if expression.len() < width * 2
        || !expression.as_bytes()[expression.len() - width..]
            .iter()
            .all(|byte| *byte == quote)
    {
        return None;
    }
    let value = &expression[width..expression.len() - width];
    if raw {
        Some(value.to_string())
    } else {
        Some(value.replace("\\'", "'").replace("\\\"", "\""))
    }
}

#[cfg(test)]
mod tests {
    use super::{invocation_arguments, string_literal_value};

    #[test]
    fn captures_named_arguments_and_map_entries() {
        let source = "path: '/home', routes: <String, WidgetBuilder>{'/': home, '/x': other}";
        let args = invocation_arguments(source, source, 0, source.len());
        assert_eq!(args[0].name.as_deref(), Some("path"));
        assert_eq!(args[0].string_value.as_deref(), Some("/home"));
        assert_eq!(args[1].map_entries.len(), 2);
        assert_eq!(args[1].map_entries[1].string_key.as_deref(), Some("/x"));
    }

    #[test]
    fn reads_raw_and_regular_string_values() {
        assert_eq!(string_literal_value("r'/raw'"), Some("/raw".to_string()));
        assert_eq!(string_literal_value("\"/home\""), Some("/home".to_string()));
    }
}
