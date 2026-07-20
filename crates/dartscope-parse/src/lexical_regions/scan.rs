use super::IdentifierToken;

pub(super) fn arrow_parameter_range(source: &str, arrow: usize) -> Option<(usize, usize, usize)> {
    let bytes = source.as_bytes();
    let previous = previous_non_whitespace(bytes, arrow)?;
    if bytes[previous] != b')' {
        return None;
    }
    let open = matching_open_delimiter(source, previous, b'(', b')')?;
    Some((open + 1, previous, open))
}

pub(super) fn arrow_expression_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut at = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => break,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => break,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' if braces == 0 => break,
            b'}' => braces -= 1,
            b',' | b';' if parens == 0 && brackets == 0 && braces == 0 => break,
            _ => {}
        }
        at += 1;
    }
    at
}

pub(super) fn following_statement_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let Some(at) = next_non_whitespace(bytes, start) else {
        return bytes.len();
    };
    if bytes[at] == b'{' {
        return matching_delimiter(source, at, b'{', b'}', bytes.len())
            .map_or(bytes.len(), |close| close + 1);
    }
    source[at..]
        .find(';')
        .map_or(bytes.len(), |relative| at + relative + 1)
}

pub(super) fn find_top_level_keyword(
    source: &str,
    start: usize,
    end: usize,
    keyword: &str,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = DelimiterDepth::default();
    let mut at = start;
    while at < end.min(bytes.len()) {
        depth.observe(bytes[at]);
        if depth.is_zero() && source[at..end].starts_with(keyword) {
            let before = at.checked_sub(1).and_then(|index| bytes.get(index));
            let after = bytes.get(at + keyword.len());
            if before.is_none_or(|byte| !is_identifier_continue(*byte))
                && after.is_none_or(|byte| !is_identifier_continue(*byte))
            {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}

pub(super) fn top_level_byte_positions(
    source: &str,
    start: usize,
    end: usize,
    target: u8,
) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut positions = Vec::new();
    let mut depth = DelimiterDepth::default();
    let mut at = start;
    while at < end.min(bytes.len()) {
        let byte = bytes[at];
        if byte == target && depth.is_zero() {
            positions.push(at);
        }
        depth.observe(byte);
        at += 1;
    }
    positions
}

pub(super) fn has_top_level_byte(source: &str, start: usize, end: usize, target: u8) -> bool {
    !top_level_byte_positions(source, start, end, target).is_empty()
}

pub(super) fn top_level_segments(
    source: &str,
    start: usize,
    end: usize,
    delimiter: u8,
) -> Vec<(usize, usize)> {
    let positions = top_level_byte_positions(source, start, end, delimiter);
    let mut segments = Vec::new();
    let mut segment_start = start;
    for at in positions {
        segments.push((segment_start, at));
        segment_start = at + 1;
    }
    segments.push((segment_start, end));
    segments
}

pub(super) fn top_level_assignment(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = DelimiterDepth::default();
    let mut at = start;
    while at < end.min(bytes.len()) {
        if bytes[at] == b'=' && depth.is_zero() {
            return Some(at);
        }
        depth.observe(bytes[at]);
        at += 1;
    }
    None
}

pub(super) fn top_level_identifiers(
    source: &str,
    start: usize,
    end: usize,
) -> Vec<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut depth = DelimiterDepth::default();
    let mut at = start;
    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if depth.is_zero() {
                tokens.push(token);
            }
            at = token.end;
            continue;
        }
        depth.observe(bytes[at]);
        at += 1;
    }
    tokens
}

pub(super) fn last_top_level_identifier(
    source: &str,
    start: usize,
    end: usize,
) -> Option<IdentifierToken<'_>> {
    top_level_identifiers(source, start, end).into_iter().last()
}

pub(super) fn contains_top_level_pattern_start(source: &str, start: usize, end: usize) -> bool {
    let bytes = source.as_bytes();
    let mut depth = DelimiterDepth::default();
    let mut at = start;
    while at < end.min(bytes.len()) {
        let byte = bytes[at];
        if depth.is_zero() && matches!(byte, b'(' | b'[' | b'{') {
            return true;
        }
        depth.observe(byte);
        at += 1;
    }
    false
}

pub(super) fn contains_receiver_formal(source: &str, start: usize, end: usize) -> bool {
    source
        .get(start..end)
        .is_some_and(|value| value.contains("this.") || value.contains("super."))
}

pub(super) fn is_binding_name(value: &str) -> bool {
    value != "_"
        && !matches!(
            value,
            "required"
                | "covariant"
                | "final"
                | "var"
                | "const"
                | "late"
                | "this"
                | "super"
                | "void"
                | "dynamic"
        )
}

pub(super) fn is_control_header(source: &str, open: usize) -> bool {
    let bytes = source.as_bytes();
    let Some(previous) = previous_non_whitespace(bytes, open) else {
        return false;
    };
    let mut start = previous;
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    matches!(
        &source[start..previous + 1],
        "if" | "for" | "while" | "switch" | "catch" | "assert"
    )
}

pub(super) fn find_keyword(source: &str, keyword: &str, start: usize) -> Option<usize> {
    let mut search = start;
    while let Some(relative) = source[search..].find(keyword) {
        let at = search + relative;
        let before = at
            .checked_sub(1)
            .and_then(|index| source.as_bytes().get(index));
        let after = source.as_bytes().get(at + keyword.len());
        if before.is_none_or(|byte| !is_identifier_continue(*byte))
            && after.is_none_or(|byte| !is_identifier_continue(*byte))
        {
            return Some(at);
        }
        search = at + keyword.len();
    }
    None
}

pub(super) fn trim_range(source: &str, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    while end > start && bytes.get(end - 1).is_some_and(u8::is_ascii_whitespace) {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

pub(super) fn matching_delimiter(
    source: &str,
    open: usize,
    opening: u8,
    closing: u8,
    limit: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&opening) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = open + 1;
    while at < limit.min(bytes.len()) {
        if bytes[at] == opening {
            depth += 1;
        } else if bytes[at] == closing {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}

fn matching_open_delimiter(source: &str, close: usize, opening: u8, closing: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(close) != Some(&closing) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = close;
    while at > 0 {
        at -= 1;
        if bytes[at] == closing {
            depth += 1;
        } else if bytes[at] == opening {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

pub(super) fn identifier_at(source: &str, start: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    if !bytes
        .get(start)
        .is_some_and(|byte| is_identifier_start(*byte))
    {
        return None;
    }
    let end = identifier_end(bytes, start);
    Some(IdentifierToken {
        text: &source[start..end],
        start,
        end,
    })
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

fn previous_non_whitespace(bytes: &[u8], before: usize) -> Option<usize> {
    let mut at = before;
    while at > 0 {
        at -= 1;
        if !bytes[at].is_ascii_whitespace() {
            return Some(at);
        }
    }
    None
}

pub(super) fn next_non_whitespace(bytes: &[u8], mut at: usize) -> Option<usize> {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    (at < bytes.len()).then_some(at)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[derive(Debug, Default)]
struct DelimiterDepth {
    parens: usize,
    brackets: usize,
    braces: usize,
}

impl DelimiterDepth {
    fn observe(&mut self, byte: u8) {
        match byte {
            b'(' => self.parens += 1,
            b')' => self.parens = self.parens.saturating_sub(1),
            b'[' => self.brackets += 1,
            b']' => self.brackets = self.brackets.saturating_sub(1),
            b'{' => self.braces += 1,
            b'}' => self.braces = self.braces.saturating_sub(1),
            _ => {}
        }
    }

    fn is_zero(&self) -> bool {
        self.parens == 0 && self.brackets == 0 && self.braces == 0
    }
}
