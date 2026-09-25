use dartscope_core::DartDiagnostic;

use crate::literals::{consume_string, string_start};
use crate::source_lines::span_for_byte_range;

// Re-export the canonical literal helpers so existing `crate::lexical::`
// call sites keep working while the implementation lives in `literals.rs`.
pub(crate) use crate::literals::{
    StringLiteralRange, find_string_literal_start, string_literal_range, string_literals_value,
};

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
    crate::identifiers::is_identifier_continue(byte)
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
