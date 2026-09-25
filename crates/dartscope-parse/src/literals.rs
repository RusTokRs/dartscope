//! Canonical Dart literal scanning.
//!
//! This module owns string and numeric literal rules so the rest of the
//! conservative parser does not duplicate them. Every scanner that needs to
//! recognise a Dart string literal (masking, directive URIs, const values,
//! invocation arguments, GraphQL documents) uses the same `string_start` /
//! `consume_string` / `string_literal_range` implementation. GraphQL names
//! are intentionally separate (`graphql.rs` keeps its own `[_A-Za-z]`
//! grammar, see `next_graphql_name`).
//!
//! Numeric literal handling is intentionally minimal in the `0.1` heuristic
//! backend (no type inference), but the character predicates are centralized
//! here so future `DS-PARSE-007` work does not reintroduce drift.
//!
//! The implementation is byte-based and ASCII-only, matching the rest of the
//! heuristic backend. Non-ASCII Dart identifiers remain out of scope until a
//! real lexer is introduced.

use crate::identifiers::is_identifier_continue;

/// Byte range of one *terminated* Dart string literal.
///
/// `content_start..content_end` is the raw content between delimiters
/// (after `r` and the opening quotes, before the closing quotes).
/// `end` is the byte index immediately after the closing delimiter.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StringLiteralRange {
    pub(crate) content_start: usize,
    pub(crate) content_end: usize,
    pub(crate) end: usize,
}

// ---------------------------------------------------------------------------
// String delimiters — raw / triple / escape handling
// ---------------------------------------------------------------------------

/// Returns the string delimiter that starts at `index`, if any.
///
/// Handles `'…'`, `"…"`, `r'…'`, `r"…"`, `'''…'''`, `r'''…'''` etc.
/// `r` is only a raw prefix when it is not part of an identifier
/// (`foo r'…'` is not a string, `foo_r'…'` is).
pub(crate) fn string_start(bytes: &[u8], index: usize) -> Option<(usize, u8, bool, bool)> {
    let (quote_index, raw) = match bytes[index] {
        b'\'' | b'"' => (index, false),
        b'r' if matches!(bytes.get(index + 1), Some(b'\'' | b'"'))
            && (index == 0 || !is_identifier_continue(bytes[index - 1])) =>
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

/// Consumes a Dart string literal that starts at `content_start`.
///
/// Returns `(next_index, terminated)`. `next_index` is the byte after the
/// closing delimiter when terminated, otherwise the end of input.
/// Triple-quoted strings may span newlines; single-quoted strings are
/// unterminated when a newline is hit. Escapes are honoured only for
/// non-raw strings.
pub(crate) fn consume_string(
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

// ---------------------------------------------------------------------------
// Public string helpers (single source of truth for the whole crate)
// ---------------------------------------------------------------------------

/// Returns the first byte that starts a terminated string literal at or
/// after `from`.
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
/// The scan follows the same rules as lexical masking, so raw strings,
/// triple quotes, and escaped quotes all resolve the way the rest of the
/// parser treats them. Unterminated literals are rejected instead of being
/// reported with a truncated value.
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

/// Concatenates the adjacent string literals that begin at `start`, as Dart
/// compiles them.
///
/// Returns the raw content between delimiters together with the byte index
/// after the last literal. Content is not unescaped: consumers receive the
/// literal's source text. This matches `DartStringConstant.value` semantics
/// (raw content as written, exact spans).
pub(crate) fn string_literals_value(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut value = String::new();
    let mut end = None;
    let mut cursor = start.min(bytes.len());
    while let Some(range) = string_literal_range(source, cursor) {
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

/// Returns the string value of a *single* literal expression such as
/// `"'/home'"` or `"r'/raw'"`.
///
/// This is the canonical helper for `invocation_arguments` and other
/// single-argument contexts. It validates that the trimmed expression is
/// exactly one terminated literal and then returns its raw content
/// (unescaped for non-raw, verbatim for raw). Adjacent concatenation
/// (`'/a' '/b'`) is intentionally *not* handled here — use
/// `string_literals_value` for that.
pub(crate) fn single_string_literal_value(expression: &str) -> Option<String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return None;
    }
    let range = string_literal_range(trimmed, 0)?;
    if range.end != trimmed.len() {
        return None;
    }
    let content = &trimmed[range.content_start..range.content_end];
    // Raw strings are verbatim; non-raw we conservatively unescape only the
    // escapes that affect asset/localization path strings. Full Dart escape
    // handling (`\n`, `\uXXXX`, `\xFF`, etc.) remains out of scope until a
    // real literal evaluator is introduced — consumers that need exact source
    // text use `string_literals_value` instead.
    let is_raw = trimmed.as_bytes().first() == Some(&b'r');
    if is_raw {
        Some(content.to_string())
    } else {
        // Preserve raw content for most escapes; only handle the two that
        // appear in pubspec asset paths and invocation strings.
        // This matches the previous ad-hoc `replace("\\'", "'")` behaviour
        // but is now centralized.
        let unescaped = content.replace("\\'", "'").replace("\\\"", "\"");
        // Collapse escaped backslashes after the above so `"\\'"` -> `\'` -> `'`
        // is not double-counted. Keep it simple: final pass for `\\`.
        Some(unescaped.replace("\\\\", "\\"))
    }
}

// ---------------------------------------------------------------------------
// Numeric literals — predicates only (evaluation out of scope)
// ---------------------------------------------------------------------------

/// Returns whether `byte` may start a Dart numeric literal.
///
/// Covers decimal (`0`, `1_000`), hex (`0xFF`), binary (`0b101`) and
/// the leading `.` of `.5` is *not* included — callers must handle that
/// as part of a larger expression.
#[allow(dead_code)]
pub(crate) fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

#[allow(dead_code)]
pub(crate) fn is_hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Returns whether `byte` may continue a numeric literal after the first
/// digit, including `_` separators and radix prefixes.
#[allow(dead_code)]
pub(crate) fn is_numeric_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.')
}

/// Returns the exclusive end of the numeric literal starting at `from`.
///
/// This is a conservative scan: it consumes `[0-9a-zA-Z_\.]` and stops at the
/// first byte that cannot be part of a number. It does not validate the
/// literal (e.g. `1__2` or `0xG` are consumed as far as they go and left for
/// the diagnostic layer). The purpose is to have a single predicate so
/// future type-aware scans do not drift.
#[allow(dead_code)]
pub(crate) fn numeric_literal_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while bytes.get(at).is_some_and(|b| is_numeric_continue(*b)) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::{
        find_string_literal_start, single_string_literal_value, string_literal_range,
        string_literals_value,
    };

    #[test]
    fn single_literal_handles_raw_and_escapes() {
        assert_eq!(single_string_literal_value(r"'/home'"), Some("/home".to_string()));
        assert_eq!(single_string_literal_value(r"r'/raw'"), Some("/raw".to_string()));
        assert_eq!(single_string_literal_value(r"'it\'s'"), Some("it's".to_string()));
        assert_eq!(single_string_literal_value("\"/a\""), Some("/a".to_string()));
        assert_eq!(single_string_literal_value("'a' 'b'"), None); // adjacent not single
    }

    #[test]
    fn concatenated_literals_via_shared_scanner() {
        let (value, end) = string_literals_value("'/a' '/b'", 0).unwrap();
        assert_eq!(value, "/a/b");
        assert_eq!(end, "'/a' '/b'".len());
        assert_eq!(string_literal_range("r'''a\nb'''", 0).unwrap().content_start, 4);
    }

    #[test]
    fn dollar_in_identifier_does_not_break_raw_detection() {
        // `foo$r'bar'` should NOT be a raw string because `$` is identifier continue
        assert!(find_string_literal_start("foo$r'bar'", 0).is_none());
        // `foo r'bar'` after space IS a raw string
        assert_eq!(find_string_literal_start("foo r'bar'", 3), Some(4));
    }
}
