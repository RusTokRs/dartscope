pub(crate) fn set_or_matches_indent(
    slot: &mut Option<usize>,
    indent: usize,
    parent_indent: usize,
) -> bool {
    if indent <= parent_indent {
        return false;
    }
    match *slot {
        Some(expected) => indent == expected,
        None => {
            *slot = Some(indent);
            true
        }
    }
}

pub(crate) fn parse_inline_sequence(value: &str) -> Option<Vec<String>> {
    let value = value.trim();
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut values = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut chars = inner.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ',' => {
                push_inline_scalar(&mut values, &inner[start..index])?;
                start = index + ch.len_utf8();
            }
            '[' | ']' | '{' | '}' => return None,
            _ => {}
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    push_inline_scalar(&mut values, &inner[start..])?;
    Some(values)
}

fn push_inline_scalar(values: &mut Vec<String>, value: &str) -> Option<()> {
    let value = value.trim();
    if value.is_empty() || yaml_key_value(value).is_some() {
        return None;
    }
    values.push(yaml_scalar(value).to_string());
    Some(())
}

pub(crate) fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let colon = find_mapping_colon(trimmed)?;
    let key = trimmed[..colon].trim();
    if key.is_empty() {
        return None;
    }
    let value = trimmed[colon + 1..].trim();
    Some((yaml_scalar(key), (!value.is_empty()).then_some(value)))
}

fn find_mapping_colon(value: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut chars = value.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ':' => {
                let is_separator = chars
                    .peek()
                    .is_none_or(|(_, next)| next.is_whitespace());
                if is_separator {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn strip_yaml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;
    let mut chars = line.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '#' if previous.is_none_or(char::is_whitespace) => return &line[..index],
                _ => {}
            }
        }
        previous = Some(ch);
    }
    line
}

pub(crate) fn yaml_scalar(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if matches!((first, last), (b'\'', b'\'') | (b'"', b'"')) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

pub(crate) fn leading_indentation_contains_tab(line: &str) -> bool {
    line.chars()
        .take_while(|ch| ch.is_whitespace())
        .any(|ch| ch == '\t')
}

pub(crate) fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}
