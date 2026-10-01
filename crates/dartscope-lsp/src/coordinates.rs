//! UTF-16 ↔ UTF-8 coordinate conversion for LSP.
//!
//! LSP positions are `(line, character)` where `line` is 0-indexed and
//! `character` is a 0-indexed offset in UTF-16 code units from the start
//! of the line. DartScope's `SourceSpan` is 1-indexed line/column in Unicode
//! scalar values (chars) with byte offsets. This module converts losslessly
//! for `\n`, `\r\n` and `\r` line endings (the three the protocol defines) and
//! non-BMP characters (surrogate pairs).
//!
//! [`LineIndex`] is built once per text and answers every conversion with a binary search over
//! its lines; the free functions are one-shot wrappers around it.

use dartscope_core::SourceSpan;

use crate::types::{Position, Range};

/// Start and content end of every line of one text.
///
/// A text always has at least one line, and a terminator at the very end opens a final empty
/// line, which is how editors count lines.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    /// `(start, content_end)` per line; the content excludes the line terminator.
    lines: Vec<(usize, usize)>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut lines = Vec::new();
        let mut start = 0;
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => {
                    lines.push((start, index));
                    index += 1;
                    start = index;
                }
                b'\r' => {
                    lines.push((start, index));
                    index += if bytes.get(index + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    start = index;
                }
                _ => index += 1,
            }
        }
        lines.push((start, bytes.len()));
        Self { text, lines }
    }

    /// Number of lines, counting the empty line after a trailing terminator.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The position of a byte offset.
    ///
    /// An offset past the end is clamped to the end, one inside a multi-byte character moves back
    /// to the start of that character, and one inside a line terminator is the end of its line.
    pub fn position(&self, offset: usize) -> Position {
        let offset = floor_char_boundary(self.text, offset.min(self.text.len()));
        let line = self.line_of(offset);
        let (start, end) = self.lines[line];
        let character = utf16_len(&self.text[start..offset.min(end)]);
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(character).unwrap_or(u32::MAX),
        }
    }

    /// The byte offset of a position, or `None` when the line or the character does not exist.
    ///
    /// A character inside a surrogate pair is the start of that character.
    pub fn offset(&self, position: Position) -> Option<usize> {
        let &(start, end) = self.lines.get(position.line as usize)?;
        let within = utf16_to_byte_offset(&self.text[start..end], position.character as usize)?;
        Some(start + within)
    }

    /// The byte offset of a position, with a position beyond the end of its line placed at the end
    /// of the line and a line beyond the text placed at the end of the text, as the protocol asks.
    pub fn offset_clamped(&self, position: Position) -> usize {
        let Some(&(start, end)) = self.lines.get(position.line as usize) else {
            return self.text.len();
        };
        utf16_to_byte_offset(&self.text[start..end], position.character as usize)
            .map_or(end, |within| start + within)
    }

    /// One-based line number and one-based character column of a byte offset.
    fn line_and_column(&self, offset: usize) -> (usize, usize) {
        let offset = floor_char_boundary(self.text, offset.min(self.text.len()));
        let line = self.line_of(offset);
        let (start, end) = self.lines[line];
        (
            line + 1,
            self.text[start..offset.min(end)].chars().count() + 1,
        )
    }

    /// Index of the line that contains `offset`; an offset inside a terminator belongs to the line
    /// that the terminator ends.
    fn line_of(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|&(start, _)| start <= offset)
            .saturating_sub(1)
    }
}

/// Converts a byte offset in `source` to an LSP `Position`.
///
/// `offset` is a byte index into `source` (0 ≤ offset ≤ source.len()).
/// If `offset` is in the middle of a UTF-8 code point, it is clamped to the
/// start of that code point. A line terminator (`\n`, `\r\n` or `\r`) belongs to the
/// line it ends and is not counted as a character.
pub fn byte_offset_to_lsp_position(source: &str, offset: usize) -> Position {
    LineIndex::new(source).position(offset)
}

/// Converts an LSP `Position` to a byte offset in `source`.
///
/// Returns `None` if the position is outside the document (line exceeds
/// line count, or character exceeds line length in UTF-16). For positions
/// at the line break (e.g. CRLF), the offset points to the start of the
/// line break.
pub fn lsp_position_to_byte_offset(source: &str, position: Position) -> Option<usize> {
    LineIndex::new(source).offset(position)
}

/// Converts a `SourceSpan` (1-indexed, char columns) to an LSP `Range` (0-indexed, UTF-16).
pub fn source_span_to_lsp_range(source: &str, span: &SourceSpan) -> Range {
    let index = LineIndex::new(source);
    Range {
        start: index.position(span.byte_start),
        end: index.position(span.byte_end),
    }
}

/// Converts an LSP `Range` to a `SourceSpan`.
///
/// Returns `None` if either endpoint is outside the document.
pub fn lsp_range_to_source_span(source: &str, range: Range) -> Option<SourceSpan> {
    let index = LineIndex::new(source);
    let start_offset = index.offset(range.start)?;
    let end_offset = index.offset(range.end)?;
    let (start_line, start_column) = index.line_and_column(start_offset);
    let (end_line, end_column) = index.line_and_column(end_offset);
    Some(SourceSpan {
        byte_start: start_offset,
        byte_end: end_offset,
        start_line,
        start_column,
        end_line,
        end_column,
    })
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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
    // A character beyond the end of the line is `None` for the strict conversion; callers that
    // need the protocol's clamping use `LineIndex::offset_clamped`.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Position;

    #[test]
    fn byte_offset_round_trips_lf() {
        let source = "a\nb\nc";
        assert_eq!(
            byte_offset_to_lsp_position(source, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 1),
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 2),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 3),
            Position {
                line: 1,
                character: 1
            }
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 0,
                    character: 1
                }
            ),
            Some(1)
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            Some(2)
        );
    }

    #[test]
    fn handles_crlf() {
        let source = "a\r\nb\r\nc";
        // "a" + "\r\n" = 3 bytes, line 0 content "a", line 1 content "b"
        assert_eq!(
            byte_offset_to_lsp_position(source, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 1),
            Position {
                line: 0,
                character: 1
            }
        );
        // bytes: 0 'a', 1 '\r', 2 '\n', 3 'b', 4 '\r', 5 '\n', 6 'c'
        // offset 2 is the '\n' of the first CRLF: it still belongs to the end of line 0
        assert_eq!(
            byte_offset_to_lsp_position(source, 2),
            Position {
                line: 0,
                character: 1
            }
        );
        // offset 3 is 'b', the first character of line 1
        assert_eq!(
            byte_offset_to_lsp_position(source, 3),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 4),
            Position {
                line: 1,
                character: 1
            }
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            Some(3)
        );
        // byte 3 is after "\r\n"?
        // Our line_content_by_index: line 0 start 0, content "a" (0..1), line 1 start 3, content "b" (3..4)
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 1,
                    character: 1
                }
            ),
            Some(4)
        );
    }

    #[test]
    fn handles_emoji_2_utf16_units() {
        let source = "a😀b"; // 'a' 1 byte, '😀' 4 bytes, 2 utf16 units, 'b' 1 byte
        // Positions: line 0, char 0 -> 'a', char1 -> start of emoji, char3 -> 'b'
        assert_eq!(
            byte_offset_to_lsp_position(source, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 1),
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            byte_offset_to_lsp_position(source, 5),
            Position {
                line: 0,
                character: 3
            }
        ); // after emoji (1+4)
        assert_eq!(
            byte_offset_to_lsp_position(source, 6),
            Position {
                line: 0,
                character: 4
            }
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 0,
                    character: 1
                }
            ),
            Some(1)
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            Some(5)
        );
        // character 2 is inside surrogate pair — clamped to start of emoji
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 0,
                    character: 2
                }
            ),
            Some(1)
        );
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let source = "ab";
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 5,
                    character: 0
                }
            ),
            None
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                source,
                Position {
                    line: 0,
                    character: 10
                }
            ),
            None
        );
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

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn a_lone_carriage_return_ends_a_line() {
        // bytes: 0 'a', 1 CR, 2 'b', 3 CR, 4 LF, 5 'c', 6 LF, 7 'd'
        let source = "a\rb\r\nc\nd";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.position(2), at(1, 0));
        assert_eq!(index.position(5), at(2, 0));
        assert_eq!(index.position(7), at(3, 0));
        assert_eq!(index.position(8), at(3, 1));
        assert_eq!(index.offset(at(1, 1)), Some(3));
    }

    #[test]
    fn a_trailing_terminator_opens_an_empty_last_line() {
        let index = LineIndex::new("a\n");
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.position(2), at(1, 0));
        assert_eq!(index.offset(at(1, 0)), Some(2));
        assert_eq!(index.offset(at(2, 0)), None);
        assert_eq!(LineIndex::new("").line_count(), 1);
    }

    #[test]
    fn clamped_offsets_follow_the_protocol() {
        let source = "ab\ncd";
        let index = LineIndex::new(source);
        // A character beyond the end of its line is the end of that line.
        assert_eq!(index.offset_clamped(at(0, 99)), 2);
        // A line beyond the text is the end of the text.
        assert_eq!(index.offset_clamped(at(9, 0)), source.len());
        assert_eq!(index.offset_clamped(at(1, 1)), 4);
        // Inside a surrogate pair is the start of the character.
        assert_eq!(LineIndex::new("a😀").offset_clamped(at(0, 2)), 1);
    }

    #[test]
    fn every_character_boundary_converts_consistently() {
        for source in [
            "",
            "x",
            "a\n",
            "a😀b\r\nc\rd\n",
            "日本語\n€ x\r\n\r\n",
            "\n\n",
        ] {
            let index = LineIndex::new(source);
            for offset in (0..=source.len()).filter(|offset| source.is_char_boundary(*offset)) {
                let position = index.position(offset);
                let back = index
                    .offset(position)
                    .expect("a position of the text exists");
                // Only the LF of a CRLF pair has no position of its own: it is the end of its line.
                assert!(
                    back == offset || back + 1 == offset,
                    "{offset} -> {position:?} -> {back} in {source:?}"
                );
                assert_eq!(index.position(back), position, "{source:?}");
            }
        }
    }

    #[test]
    fn a_range_converts_to_a_span_with_character_columns() {
        let source = "x😀y\r\nüber z\n";
        let range = Range {
            start: at(1, 0),
            end: at(1, 4),
        };
        let span = lsp_range_to_source_span(source, range).unwrap();
        assert_eq!(&source[span.byte_start..span.byte_end], "über");
        assert_eq!((span.start_line, span.start_column), (2, 1));
        assert_eq!((span.end_line, span.end_column), (2, 5));
        assert_eq!(source_span_to_lsp_range(source, &span), range);
    }
}
