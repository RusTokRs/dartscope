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
    walk_annotations(source, start, limit, |_| {})
}

/// `annotations_end` for the lines of one text, remembering the chain of annotations it last walked.
///
/// A declaration preceded by thousands of annotation lines is asked about once per line, and each
/// answer is the end of the whole run of annotations that follows. The answer from any annotation
/// of a chain is the answer from the first one, so a line that starts an annotation the previous
/// walk passed gets that walk's end without walking the rest of the run again.
pub(crate) struct AnnotationRuns {
    limit: usize,
    /// Where each annotation of the last walked chain starts, in increasing order.
    starts: Vec<usize>,
    /// What `annotations_end` returns from every position in `starts`.
    end: usize,
}

impl AnnotationRuns {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            starts: Vec::new(),
            end: 0,
        }
    }

    pub(crate) fn end(&mut self, source: &str, start: usize) -> usize {
        if self.starts.binary_search(&start).is_ok() {
            return self.end;
        }
        let mut starts = Vec::new();
        let end = walk_annotations(source, start, self.limit, |at| starts.push(at));
        if !starts.is_empty() {
            self.starts = starts;
            self.end = end;
        }
        end
    }
}

/// Walks the annotations from `start`, reporting where each of them starts.
fn walk_annotations(
    source: &str,
    start: usize,
    limit: usize,
    mut visit: impl FnMut(usize),
) -> usize {
    let bytes = source.as_bytes();
    let limit = limit.min(bytes.len());
    let mut at = start.min(limit);
    while at < limit && bytes[at] == b'@' {
        visit(at);
        let mut next = at + 1;
        let identifier_start = next;
        while next < limit
            && (bytes[next].is_ascii_alphanumeric() || matches!(bytes[next], b'_' | b'$' | b'.'))
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
    use super::{AnnotationRuns, annotations_end};

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

    #[test]
    fn remembered_chains_answer_like_a_fresh_walk_from_every_position() {
        let sources = [
            "@A\n@B<int>\n@C(a: 1,\n  b: @D(2))\nclass Foo {}\n",
            "@A\n@\nclass Foo {}\n",
            "@A\n@B(\n  1,\n  2\nclass Foo {}\n",
            "  @A @B @C\n\n  void f() {}\n@D\n",
            "class Foo {}\n@A(@B(@C()))\n@E\nint x;\n",
        ];
        for source in sources {
            for limit in [source.len(), source.len() - 6, 3, 0] {
                // Visit the positions in increasing order, like the lines of a file, and then in
                // decreasing order, which breaks every chain that the walk remembered.
                let mut positions: Vec<usize> = (0..=source.len()).collect();
                for round in 0..2 {
                    let mut runs = AnnotationRuns::new(limit);
                    for &at in &positions {
                        assert_eq!(
                            runs.end(source, at),
                            annotations_end(source, at, limit),
                            "{source:?} from {at} with limit {limit}"
                        );
                    }
                    if round == 0 {
                        positions.reverse();
                    }
                }
            }
        }
    }

    #[test]
    fn thousands_of_annotation_lines_are_walked_once() {
        let count = 50_000;
        let source = format!("{}class Foo {{}}\n", "@A\n".repeat(count));
        let mut runs = AnnotationRuns::new(source.len());
        let class_at = count * 3;
        // Each line asks from its own start; walking the rest of the run from each of them would be
        // about 3 * 10^9 byte reads.
        for line in 0..count {
            assert_eq!(runs.end(&source, line * 3), class_at);
        }
        assert_eq!(runs.end(&source, class_at), class_at);
    }
}
