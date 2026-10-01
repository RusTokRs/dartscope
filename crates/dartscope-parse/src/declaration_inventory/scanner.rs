use std::cell::Cell;

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

/// The scan that `Scans::body_range` makes under its budget, kept as the specification.
#[cfg(test)]
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

/// Brace depth at every byte offset of one text.
///
/// The depth is kept as its value after each brace, so the depth at any offset is a binary search.
/// Measuring it from the start of the offset's line instead costs the length of that line for every
/// declaration, which is quadratic for a file that declares many things on one line.
pub(super) struct BraceDepths {
    /// `(offset of a brace, depth after it)`, ordered by offset.
    after_brace: Vec<(usize, usize)>,
}

impl BraceDepths {
    pub(super) fn new(source: &str) -> Self {
        let mut after_brace = Vec::new();
        let mut depth = 0usize;
        for (offset, byte) in source.bytes().enumerate() {
            match byte {
                b'{' => {
                    depth += 1;
                    after_brace.push((offset, depth));
                }
                b'}' => {
                    depth = depth.saturating_sub(1);
                    after_brace.push((offset, depth));
                }
                _ => {}
            }
        }
        Self { after_brace }
    }

    /// The depth at `at`: the braces before it, with a closing brace never taking the depth below
    /// zero. The brace at `at` itself is not counted.
    pub(super) fn at(&self, at: usize) -> usize {
        match self.after_brace.partition_point(|&(offset, _)| offset < at) {
            0 => 0,
            count => self.after_brace[count - 1].1,
        }
    }
}

/// How many bytes the unbounded scans of one file may read, per byte of the file.
///
/// A header or a declaration is scanned to its terminator, and the scan of a line that has none
/// reads to the end of the file, so a file of thousands of such lines (imports without semicolons,
/// prose, a truncated paste) is read thousands of times over. Real code reads every byte a handful
/// of times: the scan of a type, of each member inside it and of each statement inside that. A file
/// that exceeds the factor stops being scanned and its inventory ends there, with a warning.
const SCAN_BUDGET_PER_BYTE: usize = 64;

/// The part of the scan budget that does not depend on the size of the file.
const SCAN_BUDGET_BASE_BYTES: usize = 1 << 20;

/// The unbounded scans over one masked text, and the budget that keeps their total linear.
pub(super) struct Scans<'a> {
    masked: &'a str,
    depths: BraceDepths,
    remaining: Cell<usize>,
    /// Where the scan that exhausted the budget started.
    exhausted_at: Cell<Option<usize>>,
}

impl<'a> Scans<'a> {
    pub(super) fn new(masked: &'a str) -> Self {
        Self::with_budget(
            masked,
            masked
                .len()
                .saturating_mul(SCAN_BUDGET_PER_BYTE)
                .saturating_add(SCAN_BUDGET_BASE_BYTES),
        )
    }

    fn with_budget(masked: &'a str, budget: usize) -> Self {
        Self {
            masked,
            depths: BraceDepths::new(masked),
            remaining: Cell::new(budget),
            exhausted_at: Cell::new(None),
        }
    }

    pub(super) fn depth_at(&self, at: usize) -> usize {
        self.depths.at(at)
    }

    /// The offset of the scan that used up the budget; every scan since has been refused.
    pub(super) fn exhausted_at(&self) -> Option<usize> {
        self.exhausted_at.get()
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.exhausted_at.get().is_some()
    }

    /// Takes `bytes` out of the budget for a scan that began at `at`; `false` once it is spent.
    fn charge(&self, at: usize, bytes: usize) -> bool {
        match self.remaining.get().checked_sub(bytes) {
            Some(remaining) if !self.is_exhausted() => {
                self.remaining.set(remaining);
                true
            }
            _ => {
                self.remaining.set(0);
                if !self.is_exhausted() {
                    self.exhausted_at.set(Some(at));
                }
                false
            }
        }
    }

    /// The header that starts at `at`, up to its terminator or the end of the text.
    pub(super) fn header(&self, at: usize) -> Option<&'a str> {
        if self.is_exhausted() {
            return None;
        }
        let header = declaration_header(self.masked, at)?;
        self.charge(at, header.len()).then_some(header)
    }

    /// Where the declaration that starts at `at` ends.
    pub(super) fn end(&self, at: usize, mode: EndMode) -> Option<usize> {
        if self.is_exhausted() {
            return None;
        }
        let end = declaration_end(self.masked, at, mode);
        let scanned = end.unwrap_or(self.masked.len()).saturating_sub(at);
        if self.charge(at, scanned) { end } else { None }
    }

    /// The first top-level `{` in `[at, end)` and the `}` that closes it.
    pub(super) fn body_range(&self, at: usize, end: usize) -> Option<(usize, usize)> {
        if self.is_exhausted() {
            return None;
        }
        let open = first_top_level_brace(self.masked, at, end);
        let to_open = open.unwrap_or_else(|| end.min(self.masked.len()));
        if !self.charge(at, to_open.saturating_sub(at)) {
            return None;
        }
        let open = open?;
        let close = find_matching_brace(self.masked, open);
        let to_close = close.unwrap_or(self.masked.len());
        if !self.charge(open, to_close.saturating_sub(open)) {
            return None;
        }
        Some((open, close?))
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

pub(super) use crate::metadata::AnnotationRuns;

#[cfg(test)]
mod tests {
    use super::*;

    /// The scan from the start of the text that `BraceDepths` replaces.
    fn scanned_depth(source: &str, at: usize) -> usize {
        source.as_bytes()[..at.min(source.len())]
            .iter()
            .fold(0usize, |depth, byte| match byte {
                b'{' => depth + 1,
                b'}' => depth.saturating_sub(1),
                _ => depth,
            })
    }

    #[test]
    fn the_depth_at_every_offset_matches_a_scan_from_the_start() {
        for source in [
            "",
            "{",
            "}",
            "}{",
            "a { b { c } d } e",
            "}}{{{}}}{\n{{\r\n}",
            "x{y}z}{",
        ] {
            let depths = BraceDepths::new(source);
            for at in 0..=source.len() + 2 {
                assert_eq!(
                    depths.at(at),
                    scanned_depth(source, at),
                    "{source:?} at {at}"
                );
            }
        }
    }

    #[test]
    fn the_scans_return_what_the_free_functions_return_while_the_budget_lasts() {
        let source = "class A extends B { int f() { return 1; } }\nint x = 1;\nimport 'a'\n";
        let scans = Scans::new(source);
        for at in 0..=source.len() + 1 {
            assert_eq!(scans.header(at), declaration_header(source, at), "header {at}");
            for mode in [EndMode::BodyOrSemicolon, EndMode::SemicolonOnly] {
                assert_eq!(
                    scans.end(at, mode),
                    declaration_end(source, at, mode),
                    "end {at}"
                );
            }
            for end in [at, at + 5, source.len() + 3] {
                assert_eq!(
                    scans.body_range(at, end),
                    body_range(source, at, end),
                    "body {at}..{end}"
                );
            }
        }
        assert!(!scans.is_exhausted());
    }

    #[test]
    fn lines_that_never_end_exhaust_the_budget_after_a_bounded_number_of_scans() {
        // Each header runs to the end of the text, so reading all of them is quadratic. With the
        // budget the scans stop being made after a few dozen, however many lines follow.
        let source = "import 'a.dart'\n".repeat(20_000);
        let scans = Scans::with_budget(&source, 50 * source.len());
        let mut answered = 0usize;
        for line in 0..20_000 {
            if scans.header(line * 16).is_some() {
                answered += 1;
            }
        }
        assert!(scans.is_exhausted());
        assert!((10..200).contains(&answered), "{answered} headers were answered");
        assert_eq!(scans.exhausted_at(), Some(answered * 16));
        assert_eq!(scans.end(0, EndMode::SemicolonOnly), None);
        assert_eq!(scans.body_range(0, source.len()), None);
        // Whatever was refused costs nothing more.
        assert_eq!(scans.header(0), None);
    }
}
