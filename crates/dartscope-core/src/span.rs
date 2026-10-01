//! Byte and line/column locations in a source file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SourceSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SourceSpan {
    pub fn line(line_number: usize, byte_start: usize, text: &str) -> Self {
        Self {
            byte_start,
            byte_end: byte_start + text.len(),
            start_line: line_number,
            start_column: 1,
            end_line: line_number,
            end_column: text.chars().count() + 1,
        }
    }
}
