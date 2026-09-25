//! UTF-16 ↔ UTF-8 coordinate conversion for LSP.
//!
//! LSP positions are `(line, character)` where `line` is 0-indexed and
//! `character` is a 0-indexed offset in UTF-16 code units from the start
//! of the line. DartScope's `SourceSpan` is 1-indexed line/column in Unicode
//! scalar values (chars) with byte offsets. This module converts losslessly
//! for LF, CRLF, and non-BMP characters (surrogate pairs).

use dartscope_core::SourceSpan;

use crate::types::{Position, Range};

/// Converts a byte offset in `source` to an LSP `Position`.
///
/// `offset` is a byte index into `source` (0 ≤ offset ≤ source.len()).
/// If `offset` is in the middle of a UTF-8 code point, it is clamped to the
/// start of that code point. Lines are split on `\n`; a preceding `\r` is
/// treated as part of the CRLF line break and not counted as a character.
pub fn byte_offset_to_lsp_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    // Clamp to char boundary
    let offset = floor_char_boundary(source, offset);
    let (line, character) = offset_to_line_and_utf16(source, offset);
    Position { line, character }
}

/// Converts an LSP `Position` to a byte offset in `source`.
///
/// Returns `None` if the position is outside the document (line exceeds
/// line count, or character exceeds line length in UTF-16). For positions
/// at the line break (e.g. CRLF), the offset points to the start of the
/// line break.
pub fn lsp_position_to_byte_offset(source: &str, position: Position) -> Option<usize> {
    let line = position.line as usize;
    let character = position.character as usize;
    let (line_start, line_content) = line_content_by_index(source, line)?;
    let byte_offset_in_line = utf16_to_byte_offset(line_content, character)?;
    Some(line_start + byte_offset_in_line)
}

/// Converts a `SourceSpan` (1-indexed, char columns) to an LSP `Range` (0-indexed, UTF-16).
pub fn source_span_to_lsp_range(source: &str, span: &SourceSpan) -> Range {
    let start = byte_offset_to_lsp_position(source, span.byte_start);
    let end = byte_offset_to_lsp_position(source, span.byte_end);
    Range { start, end }
}

/// Converts an LSP `Range` to a `SourceSpan`.
///
/// Returns `None` if either endpoint is outside the document.
pub fn lsp_range_to_source_span(source: &str, range: Range) -> Option<SourceSpan> {
    let start_offset = lsp_position_to_byte_offset(source, range.start)?;
    let end_offset = lsp_position_to_byte_offset(source, range.end)?;
    // Convert LSP 0-indexed line/utf16 to 1-indexed line/char column for SourceSpan
    let start_line = range.start.line + 1;
    let end_line = range.end.line + 1;
    // Columns are 1-indexed char counts; we approximate via byte offset char count.
    // For exact column we count chars from line start to offset.
    let start_column = byte_offset_to_char_column(source, start_offset);
    let end_column = byte_offset_to_char_column(source, end_offset);
    Some(SourceSpan {
        byte_start: start_offset,
        byte_end: end_offset,
        start_line: start_line as usize,
        start_column,
        end_line: end_line as usize,
        end_column,
    })
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn offset_to_line_and_utf16(source: &str, offset: usize) -> (u32, u32) {
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < source.len() {
        if bytes[i] == b'\n' {
            let line_end = if i > 0 && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            if offset <= line_end {
                let character = utf16_len(&source[line_start..offset.min(line_end)]) as u32;
                return (line, character);
            }
            if offset <= i {
                // offset is inside CRLF (\r) or at \n
                let character = utf16_len(&source[line_start..line_end]) as u32;
                return (line, character);
            }
            line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    // Last line (no trailing \n)
    let character = utf16_len(&source[line_start..offset]) as u32;
    (line, character)
}

fn line_content_by_index(source: &str, target_line: usize) -> Option<(usize, &str)> {
    let bytes = source.as_bytes();
    let mut line: usize = 0;
    let mut line_start: usize = 0;
    let mut i = 0;
    while i <= source.len() {
        let is_end = i == source.len();
        let is_nl = !is_end && bytes[i] == b'\n';
        if is_end || is_nl {
            let line_end = if is_nl && i > 0 && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            if line == target_line {
                return Some((line_start, &source[line_start..line_end]));
            }
            if is_end {
                break;
            }
            line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    None
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(|c| c.len_utf16()).sum()
}

fn utf16_to_byte_offset(line_content: &str, utf16_offset: usize) -> Option<usize> {
    if utf16_offset == 0 {
        return Some(0);
    }
    let mut utf16_count = 0usize;
    let mut byte_offset = 0usize;
    for c in line_content.chars() {
        let len = c.len_utf16();
        if utf16_count + len > utf16_offset {
            // Requested offset is inside a surrogate pair (e.g. middle of emoji) — clamp to char start
            return Some(byte_offset);
        }
        utf16_count += len;
        byte_offset += c.len_utf8();
        if utf16_count == utf16_offset {
            return Some(byte_offset);
        }
    }
    if utf16_count == utf16_offset {
        Some(byte_offset)
    } else {
        // Character exceeds line length — per LSP spec, positions beyond line length are clamped to line end,
        // but for strict conversion we return None to signal out-of-bounds. Caller may clamp.
        None
    }
}

fn byte_offset_to_char_column(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    let offset = floor_char_boundary(source, offset);
    // Find line start
    let line_start = source[..offset]
        .rfind('\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let column_chars = source[line_start..offset].chars().count();
    column_chars + 1 // 1-indexed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Position;

    #[test]
    fn byte_offset_round_trips_lf() {
        let source = "a\nb\nc";
        assert_eq!(byte_offset_to_lsp_position(source, 0), Position { line: 0, character: 0 });
        assert_eq!(byte_offset_to_lsp_position(source, 1), Position { line: 0, character: 1 });
        assert_eq!(byte_offset_to_lsp_position(source, 2), Position { line: 1, character: 0 });
        assert_eq!(byte_offset_to_lsp_position(source, 3), Position { line: 1, character: 1 });
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 0, character: 1 }), Some(1));
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 1, character: 0 }), Some(2));
    }

    #[test]
    fn handles_crlf() {
        let source = "a\r\nb\r\nc";
        // "a" + "\r\n" = 3 bytes, line 0 content "a", line 1 content "b"
        assert_eq!(byte_offset_to_lsp_position(source, 0), Position { line: 0, character: 0 });
        assert_eq!(byte_offset_to_lsp_position(source, 1), Position { line: 0, character: 1 });
        // offset 1 is 'a' end, offset 2 is '\r', offset 3 is '\n' — both should map to line 0 end
        assert_eq!(byte_offset_to_lsp_position(source, 3), Position { line: 0, character: 1 });
        assert_eq!(byte_offset_to_lsp_position(source, 4), Position { line: 1, character: 0 });
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 1, character: 0 }), Some(3));
        // byte 3 is after "\r\n"?
        // Our line_content_by_index: line 0 start 0, content "a" (0..1), line 1 start 3, content "b" (3..4)
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 1, character: 1 }), Some(4));
    }

    #[test]
    fn handles_emoji_2_utf16_units() {
        let source = "a😀b"; // 'a' 1 byte, '😀' 4 bytes, 2 utf16 units, 'b' 1 byte
        // Positions: line 0, char 0 -> 'a', char1 -> start of emoji, char3 -> 'b'
        assert_eq!(byte_offset_to_lsp_position(source, 0), Position { line: 0, character: 0 });
        assert_eq!(byte_offset_to_lsp_position(source, 1), Position { line: 0, character: 1 });
        assert_eq!(byte_offset_to_lsp_position(source, 5), Position { line: 0, character: 3 }); // after emoji (1+4)
        assert_eq!(byte_offset_to_lsp_position(source, 6), Position { line: 0, character: 4 });
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 0, character: 1 }), Some(1));
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 0, character: 3 }), Some(5));
        // character 2 is inside surrogate pair — clamped to start of emoji
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 0, character: 2 }), Some(1));
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let source = "ab";
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 5, character: 0 }), None);
        assert_eq!(lsp_position_to_byte_offset(source, Position { line: 0, character: 10 }), None);
    }

    #[test]
    fn source_span_round_trip() {
        let source = "class A {}\nvoid foo() {}\n";
        let span = SourceSpan {
            byte_start: 0,
            byte_end: 10,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 11,
        };
        let range = source_span_to_lsp_range(source, &span);
        let back = lsp_range_to_source_span(source, range).unwrap();
        assert_eq!(back.byte_start, span.byte_start);
        assert_eq!(back.byte_end, span.byte_end);
    }
}
