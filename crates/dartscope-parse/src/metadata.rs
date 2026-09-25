//! Canonical Dart metadata (annotation) scanning.
//!
//! Annotations in Dart are `@` followed by a dotted identifier, optional
//! type arguments `<T>` and an optional argument list `(...)`. They may span
//! multiple source lines and may appear stacked (`@A @B void f()`). Every
//! scanner that needs to skip metadata before a declaration uses this module
//! so the character class for annotation names (`[A-Za-z0-9_$]`) and the
//! handling of nested `<…>` / `(…)` groups stays consistent.
//!
//! The functions are byte-based and operate on the masked source (comments
//! and strings already replaced by spaces) so they never misinterpret a
//! `@` inside a string literal.

/// Returns the first byte after any leading metadata annotations and their
/// trailing whitespace.
///
/// Annotations may span source lines, so the returned position can be beyond
/// the caller's line. When no complete annotation is present at `start` the
/// position is returned unchanged, and a malformed annotation yields `limit`
/// so the caller never parses a partial annotation as a declaration.
pub(crate) fn annotations_end(source: &str, start: usize, limit: usize) -> usize {
    let bytes = source.as_bytes();
    let limit = limit.min(bytes.len());
    let mut at = start.min(limit);
    while at < limit && bytes[at] == b'@' {
        let mut next = at + 1;
        let identifier_start = next;
        while next < limit
            && (bytes[next].is_ascii_alphanumeric()
                || matches!(bytes[next], b'_' | b'$' | b'.'))
        {
            next += 1;
        }
        if next == identifier_start {
            return at;
        }
        at = next;
        if let Some(close) = matching_angle(source, at, limit) {
            at = close + 1;
        }
        if at < limit && bytes[at] == b'(' {
            let Some(close) = matching_close(source, at, limit) else {
                return limit;
            };
            at = close + 1;
        }
        while at < limit && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
    }
    at
}

fn matching_angle(source: &str, open: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&b'<') {
        return None;
    }
    let limit = limit.min(bytes.len());
    let mut depth = 0usize;
    for (offset, byte) in bytes[open..limit].iter().copied().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b';' | b'=' => return None,
            _ => {}
        }
    }
    None
}

/// Returns the byte index of the delimiter closing the group opened at `open`.
fn matching_close(source: &str, open: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let limit = limit.min(bytes.len());
    let mut depth = 0usize;
    let mut index = open;
    while index < limit {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::annotations_end;

    #[test]
    fn skips_dollar_decorated_annotations() {
        let source = "@_$MyAnnotation<int>(a: 1)  void f() {}";
        let end = annotations_end(source, 0, source.len());
        assert!(source[end..].starts_with("void"));
    }

    #[test]
    fn handles_stacked_and_multiline_annotations() {
        let source = "@A\n@B<int>\n@C(a: 1, b: 2) class Foo {}";
        let end = annotations_end(source, 0, source.len());
        assert!(source[end..].trim_start().starts_with("class"));
    }
}
