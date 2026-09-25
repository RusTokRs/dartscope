use dartscope_core::DartDiagnostic;

use crate::source_lines::span_for_byte_range;

pub(crate) struct LexicalMask {
    pub(crate) code: String,
    pub(crate) diagnostics: Vec<DartDiagnostic>,
}

pub(crate) fn mask_non_code(source: &str) -> LexicalMask {
    let bytes = source.as_bytes();
    let mut code = bytes.to_vec();
    let mut diagnostics = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            let start = index;
            index = consume_to_line_end(bytes, index + 2);
            mask_range(&mut code, start, index);
        } else if bytes[index..].starts_with(b"/*") {
            let start = index;
            let (next, terminated) = consume_block_comment(bytes, index + 2);
            index = next;
            if !terminated {
                diagnostics.push(unterminated_diagnostic(
                    "unterminated_block_comment",
                    "Dart block comment is not terminated",
                    source,
                    start,
                ));
            }
            mask_range(&mut code, start, index);
        } else if let Some((content_start, quote, triple, raw)) = string_start(bytes, index) {
            let start = index;
            let (next, terminated) = consume_string(bytes, content_start, quote, triple, raw);
            index = next;
            if !terminated {
                diagnostics.push(unterminated_diagnostic(
                    "unterminated_string",
                    "Dart string literal is not terminated",
                    source,
                    start,
                ));
            }
            mask_range(&mut code, start, index);
        } else {
            index += 1;
        }
    }

    LexicalMask {
        code: String::from_utf8(code).expect("masking preserves UTF-8 validity"),
        diagnostics,
    }
}

fn string_start(bytes: &[u8], index: usize) -> Option<(usize, u8, bool, bool)> {
    let (quote_index, raw) = match bytes[index] {
        b'\'' | b'"' => (index, false),
        b'r' if matches!(bytes.get(index + 1), Some(b'\'' | b'"'))
            && (index == 0 || !is_identifier_byte(bytes[index - 1])) =>
        {
            (index + 1, true)
        }
        _ => return None,
    };
    let quote = bytes[quote_index];
    let triple = bytes[quote_index..].starts_with(&[quote, quote, quote]);
    let content_start = quote_index + if triple { 3 } else { 1 };
    Some((content_start, quote, triple, raw))
}

fn consume_to_line_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
        index += 1;
    }
    index
}

fn consume_block_comment(bytes: &[u8], mut index: usize) -> (usize, bool) {
    let mut depth = 1usize;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return (index, true);
            }
        } else {
            index += 1;
        }
    }
    (index, false)
}

fn consume_string(
    bytes: &[u8],
    mut index: usize,
    quote: u8,
    triple: bool,
    raw: bool,
) -> (usize, bool) {
    while index < bytes.len() {
        if triple && bytes[index..].starts_with(&[quote, quote, quote]) {
            return (index + 3, true);
        }
        if !triple && bytes[index] == quote {
            return (index + 1, true);
        }
        if !triple && matches!(bytes[index], b'\n' | b'\r') {
            return (index, false);
        }
        if !raw && bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
        } else {
            index += 1;
        }
    }
    (index, false)
}

fn mask_range(code: &mut [u8], start: usize, end: usize) {
    for byte in &mut code[start..end] {
        if !matches!(*byte, b'\n' | b'\r') {
            *byte = b' ';
        }
    }
}

fn unterminated_diagnostic(
    code: &'static str,
    message: &'static str,
    source: &str,
    start: usize,
) -> DartDiagnostic {
    DartDiagnostic::warning(
        code,
        message,
        Some(span_for_byte_range(source, start, source.len())),
    )
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::mask_non_code;

    #[test]
    fn preserves_code_around_strings() {
        let source = "import 'src/a.dart';\nclient.query(gql(operation));";
        let mask = mask_non_code(source);
        assert_eq!(
            mask.code,
            "import             ;\nclient.query(gql(operation));"
        );
    }

    #[test]
    fn preserves_conditional_import_code() {
        let source = "import 'src/stub.dart'\n  if (dart.library.io) 'src/io.dart'\n  if (dart.library.js_interop) 'src/web.dart' show PlatformApi;";
        let mask = mask_non_code(source);
        assert!(
            mask.code.trim_start().starts_with("import "),
            "{}",
            mask.code
        );
        assert!(mask.code.contains("PlatformApi;"), "{}", mask.code);
    }
}

/// Byte ranges of one terminated Dart string literal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StringLiteralRange {
    /// First byte of the literal content, after `r` and the opening delimiter.
    pub(crate) content_start: usize,
    /// Byte index of the closing delimiter, so `source[content_start..content_end]` is the raw content.
    pub(crate) content_end: usize,
    /// Byte index directly after the closing delimiter.
    pub(crate) end: usize,
}

/// Returns the first byte that starts a terminated string literal at or after `from`.
pub(crate) fn find_string_literal_start(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = from.min(bytes.len());
    while index < bytes.len() {
        if string_start(bytes, index).is_some() && string_literal_range(source, index).is_some() {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Locates the terminated string literal starting at `start`.
///
/// The scan follows the same rules as lexical masking, so raw strings, triple quotes, and escaped
/// quotes all resolve the way the rest of the parser treats them. Unterminated literals are rejected
/// instead of being reported with a truncated value.
pub(crate) fn string_literal_range(source: &str, start: usize) -> Option<StringLiteralRange> {
    let bytes = source.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    let (content_start, quote, triple, raw) = string_start(bytes, start)?;
    let (next, terminated) = consume_string(bytes, content_start, quote, triple, raw);
    if !terminated {
        return None;
    }
    let delimiter = if triple { 3 } else { 1 };
    Some(StringLiteralRange {
        content_start,
        content_end: next.checked_sub(delimiter)?,
        end: next,
    })
}

/// Concatenates the adjacent string literals that begin at `start`, as Dart compiles them.
///
/// Returns the raw content between delimiters together with the byte index after the last literal.
/// Content is not unescaped: consumers receive the literal's source text.
pub(crate) fn string_literals_value(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut value = String::new();
    let mut end = None;
    let mut cursor = start.min(bytes.len());
    loop {
        let Some(range) = string_literal_range(source, cursor) else {
            break;
        };
        value.push_str(&source[range.content_start..range.content_end]);
        end = Some(range.end);
        let mut next = range.end;
        while next < bytes.len() && bytes[next].is_ascii_whitespace() {
            next += 1;
        }
        cursor = next;
    }
    end.map(|end| (value, end))
}
