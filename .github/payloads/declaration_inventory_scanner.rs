use crate::source_lines::SourceLine;

pub(super) fn declaration_header(source: &str, start: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => return Some(&source[start..index + 1]),
            b'=' if parens == 0 && brackets == 0 && bytes.get(index + 1) == Some(&b'>') => {
                return Some(&source[start..index + 2]);
            }
            _ => {}
        }
        index += 1;
    }
    (start < source.len()).then(|| &source[start..])
}

#[derive(Clone, Copy)]
pub(super) enum EndMode {
    BodyOrSemicolon,
    SemicolonOnly,
}

pub(super) fn declaration_end(source: &str, start: usize, mode: EndMode) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => {
                if matches!(mode, EndMode::BodyOrSemicolon)
                    && parens == 0
                    && brackets == 0
                    && braces == 0
                {
                    return find_matching_brace(source, index).map(|end| end + 1);
                }
                braces += 1;
            }
            b'}' => braces = braces.saturating_sub(1),
            b';' if parens == 0 && brackets == 0 && braces == 0 => return Some(index + 1),
            _ => {}
        }
        index += 1;
    }
    None
}

pub(super) fn body_range(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let open = first_top_level_brace(source, start, end)?;
    let close = find_matching_brace(source, open)?;
    Some((open, close))
}

fn first_top_level_brace(source: &str, start: usize, end: usize) -> Option<usize> {
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
            b'{' if parens == 0 && brackets == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[open..].iter().copied().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn enum_member_start(
    source: &str,
    body_start: usize,
    body_end: usize,
    owner_depth: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = owner_depth;
    for (index, byte) in bytes
        .iter()
        .copied()
        .enumerate()
        .take(body_end.min(bytes.len()))
        .skip(body_start + 1)
    {
        match byte {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b';' if depth == owner_depth => return Some(index + 1),
            _ => {}
        }
    }
    None
}

pub(super) fn line_brace_depths(source: &str, lines: &[SourceLine<'_>]) -> Vec<usize> {
    let mut depths = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    let mut cursor = 0usize;
    for line in lines {
        while cursor < line.byte_start {
            match source.as_bytes()[cursor] {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            cursor += 1;
        }
        depths.push(depth);
        while cursor <= line.byte_end() && cursor < source.len() {
            match source.as_bytes()[cursor] {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            cursor += 1;
        }
    }
    depths
}

pub(super) fn brace_depth_at(source: &str, at: usize) -> usize {
    source.as_bytes()[..at.min(source.len())]
        .iter()
        .fold(0usize, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth.saturating_sub(1),
            _ => depth,
        })
}

pub(super) fn first_code_byte(line: SourceLine<'_>, source: &str) -> usize {
    let text = &source[line.byte_start..line.byte_end()];
    line.byte_start + text.len().saturating_sub(text.trim_start().len())
}

pub(super) fn source_line_text<'a>(source: &'a str, line: SourceLine<'_>) -> &'a str {
    &source[line.byte_start..line.byte_end()]
}
