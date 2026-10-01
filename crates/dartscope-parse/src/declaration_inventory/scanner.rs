use crate::source_lines::SourceLine;

/// How many bytes of a statement are examined to decide whether it declares a local variable.
///
/// A declaration announces itself at its start, so a statement that spans thousands of lines
/// without looking like one (a call with a huge argument list, say) is rejected from this prefix
/// instead of being rescanned to its end from every one of its lines.
pub(super) const STATEMENT_PROBE_BYTES: usize = 2048;

pub(super) fn declaration_header(source: &str, start: usize) -> Option<&str> {
    declaration_header_within(source, start, usize::MAX).map(|(header, _)| header)
}

/// Scans the header that starts at `start`, looking at no more than `limit` bytes.
///
/// The flag is `false` when the limit ended the scan before the header's terminator, in which case
/// the text is only a prefix of the header.
pub(super) fn declaration_header_within(
    source: &str,
    start: usize,
    limit: usize,
) -> Option<(&str, bool)> {
    let bytes = source.as_bytes();
    let stop = start.saturating_add(limit).min(bytes.len());
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut index = start;
    let mut terminator_end = None;
    while index < stop {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => {
                terminator_end = Some(index + 1);
                break;
            }
            b'=' if parens == 0 && brackets == 0 && bytes.get(index + 1) == Some(&b'>') => {
                terminator_end = Some(index + 2);
                break;
            }
            _ => {}
        }
        index += 1;
    }
    #[cfg(test)]
    SCANNED_HEADER_BYTES.with(|scanned| scanned.set(scanned.get() + index.saturating_sub(start)));

    if let Some(end) = terminator_end {
        return Some((&source[start..end], true));
    }
    if start >= bytes.len() {
        return None;
    }
    if stop == bytes.len() {
        return Some((&source[start..], true));
    }
    let mut cut = stop;
    while !source.is_char_boundary(cut) {
        cut -= 1;
    }
    Some((&source[start..cut], false))
}

#[cfg(test)]
thread_local! {
    static SCANNED_HEADER_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Total number of bytes the header scans of this thread have examined so far.
#[cfg(test)]
pub(super) fn scanned_header_bytes() -> usize {
    SCANNED_HEADER_BYTES.with(std::cell::Cell::get)
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

/// Returns the brace depth at `at` from the per-line depths instead of rescanning from the file start.
///
/// `line_depths[i]` is the depth at the start of `lines[i]`. An offset that is not inside any line
/// (inside a line terminator or past the last line) falls back to a scan from the file start.
pub(super) fn depth_at(
    source: &str,
    lines: &[SourceLine<'_>],
    line_depths: &[usize],
    at: usize,
) -> usize {
    let index = lines.partition_point(|line| line.byte_end() < at);
    match (lines.get(index), line_depths.get(index)) {
        (Some(line), Some(depth)) if line.byte_start <= at => {
            depth_within_line(source, *line, *depth, at)
        }
        _ => brace_depth_at(source, at),
    }
}

pub(super) fn first_code_byte(line: SourceLine<'_>, source: &str) -> usize {
    let text = &source[line.byte_start..line.byte_end()];
    line.byte_start + text.len().saturating_sub(text.trim_start().len())
}

/// Returns the next non-whitespace byte in `source[start..end]`.
///
/// Masked comment and string bytes are spaces, so this walks over them without skipping real code.
pub(super) fn next_code_byte(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let limit = end.min(bytes.len());
    let mut index = start.min(limit);
    while index < limit {
        if !bytes[index].is_ascii_whitespace() {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Returns the brace depth at `at` given the depth measured at the start of `line`.
///
/// The position may be anywhere inside the line, so declarations that do not start a source line are
/// measured exactly like line-leading declarations.
pub(super) fn depth_within_line(
    source: &str,
    line: SourceLine<'_>,
    line_depth: usize,
    at: usize,
) -> usize {
    let start = line.byte_start.min(source.len());
    let end = at.clamp(line.byte_start, line.byte_end().min(source.len()));
    source.as_bytes()[start..end]
        .iter()
        .fold(line_depth, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth.saturating_sub(1),
            _ => depth,
        })
}

pub(super) use crate::metadata::annotations_end;
