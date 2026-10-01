use std::cell::RefCell;
use std::marker::PhantomData;

use dartscope_core::{DartDiagnostic, SourceSpan};

/// The UTF-8 byte-order mark. Dart tooling treats it as an invisible preamble of the file rather
/// than as source text, so no line starts with it and no column counts it.
const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Clone, Copy)]
pub(crate) struct SourceLine<'a> {
    pub(crate) number: usize,
    pub(crate) text: &'a str,
    pub(crate) byte_start: usize,
}

impl SourceLine<'_> {
    pub(crate) fn byte_end(self) -> usize {
        self.byte_start + self.text.len()
    }
}

/// Splits `source` into lines without their `\n` / `\r\n` terminators.
///
/// A leading byte-order mark is skipped: the first line starts after it, so declarations and
/// directives on line one are scanned exactly like those on any other line.
pub(crate) fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let preamble = if source.starts_with(BYTE_ORDER_MARK) {
        BYTE_ORDER_MARK.len_utf8()
    } else {
        0
    };
    let mut byte_start = preamble;
    source[preamble..]
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, segment)| {
            let text = segment.strip_suffix('\n').unwrap_or(segment);
            let text = text.strip_suffix('\r').unwrap_or(text);
            let line = SourceLine {
                number: index + 1,
                text,
                byte_start,
            };
            byte_start += segment.len();
            line
        })
        .collect()
}

/// Start and end (without the line terminator) byte offsets of every line of one source text.
///
/// Line starts and ends increase strictly, so a line can be found with a binary search instead of
/// a scan over all lines.
struct LineTable {
    bounds: Vec<(usize, usize)>,
}

impl LineTable {
    fn new(source: &str) -> Self {
        #[cfg(test)]
        BUILDS.with(|builds| builds.set(builds.get() + 1));
        Self {
            bounds: source_lines(source)
                .into_iter()
                .map(|line| (line.byte_start, line.byte_end()))
                .collect(),
        }
    }

    /// Index of the first line that ends at or after `byte`; the line count when there is none.
    fn first_line_ending_at_or_after(&self, byte: usize) -> usize {
        self.bounds.partition_point(|&(_, end)| end < byte)
    }

    /// One-based line number and start offset of the line at `index`.
    fn line_at(&self, index: usize) -> Option<(usize, usize)> {
        self.bounds.get(index).map(|&(start, _)| (index + 1, start))
    }
}

/// A line table that is valid for exactly one text, identified by its address and length.
struct ScopedLineTable {
    address: usize,
    len: usize,
    table: LineTable,
}

impl ScopedLineTable {
    fn covers(&self, source: &str) -> bool {
        self.address == source.as_ptr() as usize && self.len == source.len()
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<ScopedLineTable>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Number of line tables built on this thread so far (test builds only).
#[cfg(test)]
pub(crate) fn line_table_builds() -> usize {
    BUILDS.with(std::cell::Cell::get)
}

/// Keeps the line table of one source text alive for the duration of an analysis.
///
/// Every span helper in this module needs the line table of the text it measures. Without a scope
/// the table is rebuilt for each call, which makes a file with `n` declarations cost `O(n²)`. Entering
/// a scope builds the table once; `span_for_byte_range` and `line_span_for_byte` then find a line with
/// a binary search. The table is only used for the exact text it was built from (same address and
/// length), so lookups on any other text keep working unchanged, and the scope borrows the text so it
/// cannot be edited while the table is in use. Dropping the scope restores the previous one.
pub(crate) struct LineIndexScope<'a> {
    previous: Option<ScopedLineTable>,
    _source: PhantomData<&'a str>,
}

impl<'a> LineIndexScope<'a> {
    pub(crate) fn enter(source: &'a str) -> Self {
        let scoped = ScopedLineTable {
            address: source.as_ptr() as usize,
            len: source.len(),
            table: LineTable::new(source),
        };
        let previous = ACTIVE.with(|active| active.borrow_mut().replace(scoped));
        Self {
            previous,
            _source: PhantomData,
        }
    }
}

impl Drop for LineIndexScope<'_> {
    fn drop(&mut self) {
        let previous = self.previous.take();
        ACTIVE.with(|active| *active.borrow_mut() = previous);
    }
}

fn with_line_table<T>(source: &str, lookup: impl FnOnce(&LineTable) -> T) -> T {
    ACTIVE.with(|active| {
        let scoped = active.borrow();
        if let Some(scoped) = scoped.as_ref().filter(|scoped| scoped.covers(source)) {
            return lookup(&scoped.table);
        }
        drop(scoped);
        lookup(&LineTable::new(source))
    })
}

pub(crate) fn span_for_byte_range(source: &str, byte_start: usize, byte_end: usize) -> SourceSpan {
    with_line_table(source, |table| {
        let start_index = table.first_line_ending_at_or_after(byte_start);
        let start = table.line_at(start_index).unwrap_or((1, 0));
        let end_index = table.first_line_ending_at_or_after(byte_end);
        let end = table
            .line_at(end_index)
            .or_else(|| table.bounds.len().checked_sub(1).and_then(|last| table.line_at(last)))
            .unwrap_or(start);
        SourceSpan {
            byte_start,
            byte_end,
            start_line: start.0,
            start_column: column_after(source, start.1, byte_start),
            end_line: end.0,
            end_column: column_after(source, end.1, byte_end),
        }
    })
}

pub(crate) fn line_span_for_byte(source: &str, at: usize) -> SourceSpan {
    with_line_table(source, |table| {
        let index = table.first_line_ending_at_or_after(at);
        match table.bounds.get(index) {
            Some(&(start, end)) if start <= at => {
                SourceSpan::line(index + 1, start, &source[start..end])
            }
            _ => SourceSpan::line(1, 0, ""),
        }
    })
}

/// One-based column of `byte`, counted in characters from the start of its line.
///
/// An offset that is not inside the line it was matched to (for example one that points into a
/// `\r\n` terminator) is reported at column one instead of slicing backwards.
fn column_after(source: &str, line_start: usize, byte: usize) -> usize {
    source
        .get(line_start..byte)
        .map_or(1, |prefix| prefix.chars().count() + 1)
}

pub(crate) fn attach_diagnostic_paths(diagnostics: &mut [DartDiagnostic], path: &str) {
    for diagnostic in diagnostics {
        if diagnostic.path.is_none() {
            diagnostic.path = Some(path.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pre-index algorithm: a linear search over freshly split lines.
    fn reference_span(source: &str, byte_start: usize, byte_end: usize) -> SourceSpan {
        let lines = source_lines(source);
        let start = lines
            .iter()
            .copied()
            .find(|line| byte_start <= line.byte_end())
            .unwrap_or(SourceLine {
                number: 1,
                text: "",
                byte_start: 0,
            });
        let end = lines
            .iter()
            .copied()
            .find(|line| byte_end <= line.byte_end())
            .or_else(|| lines.last().copied())
            .unwrap_or(start);
        SourceSpan {
            byte_start,
            byte_end,
            start_line: start.number,
            start_column: source
                .get(start.byte_start..byte_start)
                .map_or(1, |p| p.chars().count() + 1),
            end_line: end.number,
            end_column: source
                .get(end.byte_start..byte_end)
                .map_or(1, |p| p.chars().count() + 1),
        }
    }

    fn reference_line_span(source: &str, at: usize) -> SourceSpan {
        let line = source_lines(source)
            .into_iter()
            .find(|line| line.byte_start <= at && at <= line.byte_end())
            .unwrap_or(SourceLine {
                number: 1,
                text: "",
                byte_start: 0,
            });
        SourceSpan::line(line.number, line.byte_start, line.text)
    }

    const SAMPLES: &[&str] = &[
        "",
        "a",
        "\n",
        "one\ntwo\nthree",
        "one\ntwo\nthree\n",
        "one\r\ntwo\r\n\r\nfour\r\n",
        "ключ = 'значение';\nclass Ж {}\n",
        "emoji 😀 line\nnext 😀😀 line\n",
        "\u{feff}class First {}\nclass Second {}\n",
        "\u{feff}",
        "\u{feff}\r\nx",
    ];

    #[test]
    fn indexed_lookup_matches_the_linear_search_for_every_offset() {
        for source in SAMPLES {
            let boundaries: Vec<usize> = (0..=source.len())
                .filter(|index| source.is_char_boundary(*index))
                .collect();
            let _scope = LineIndexScope::enter(source);
            for &start in &boundaries {
                assert_eq!(
                    line_span_for_byte(source, start),
                    reference_line_span(source, start),
                    "line span of {start} in {source:?}"
                );
                for &end in boundaries.iter().filter(|end| **end >= start) {
                    assert_eq!(
                        span_for_byte_range(source, start, end),
                        reference_span(source, start, end),
                        "span {start}..{end} in {source:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn unscoped_lookup_matches_the_scoped_lookup() {
        for source in SAMPLES {
            let unscoped: Vec<_> = (0..=source.len())
                .filter(|index| source.is_char_boundary(*index))
                .map(|index| span_for_byte_range(source, index, source.len()))
                .collect();
            let _scope = LineIndexScope::enter(source);
            let scoped: Vec<_> = (0..=source.len())
                .filter(|index| source.is_char_boundary(*index))
                .map(|index| span_for_byte_range(source, index, source.len()))
                .collect();
            assert_eq!(scoped, unscoped, "{source:?}");
        }
    }

    #[test]
    fn a_scope_only_serves_the_text_it_was_built_from() {
        let first = String::from("alpha\nbeta\n");
        let second = String::from("x\ny\nz\nw\n");
        let _scope = LineIndexScope::enter(&first);
        let other = span_for_byte_range(&second, 4, 5);
        assert_eq!(other, reference_span(&second, 4, 5));
        assert_eq!(other.start_line, 3);
    }

    #[test]
    fn dropping_a_nested_scope_restores_the_outer_one() {
        let outer = String::from("a\nb\nc\n");
        let inner = String::from("a\nb\nc\nd\ne\n");
        let _outer = LineIndexScope::enter(&outer);
        let before = line_table_builds();
        {
            let _inner = LineIndexScope::enter(&inner);
            assert_eq!(span_for_byte_range(&inner, 8, 9).start_line, 5);
        }
        let builds_after_inner = line_table_builds();
        assert_eq!(span_for_byte_range(&outer, 4, 5).start_line, 3);
        assert_eq!(
            line_table_builds(),
            builds_after_inner,
            "the outer table must still be active after the inner scope ended"
        );
        assert_eq!(builds_after_inner, before + 1);
    }

    #[test]
    fn a_scope_builds_the_line_table_once() {
        let source = "line\n".repeat(200);
        let _scope = LineIndexScope::enter(&source);
        let before = line_table_builds();
        for index in 0..200 {
            let _ = span_for_byte_range(&source, index * 5, index * 5 + 4);
            let _ = line_span_for_byte(&source, index * 5);
        }
        assert_eq!(line_table_builds(), before);
    }

    #[test]
    fn the_byte_order_mark_is_not_part_of_the_first_line() {
        let source = "\u{feff}class A {}\nclass B {}\n";
        let lines = source_lines(source);
        assert_eq!(lines[0].text, "class A {}");
        assert_eq!(lines[0].byte_start, 3);
        assert_eq!(lines[1].byte_start, 3 + "class A {}\n".len());

        let span = span_for_byte_range(source, 3, 13);
        assert_eq!((span.start_line, span.start_column), (1, 1));
        assert_eq!((span.end_line, span.end_column), (1, 11));
    }

    #[test]
    fn an_offset_inside_a_crlf_terminator_does_not_panic() {
        let source = "ab\r\ncd\r\n";
        // Offset 3 is the `\n` of the first terminator: the first line ends at 2.
        let span = span_for_byte_range(source, 3, 2);
        assert_eq!(span.start_line, 2);
        assert_eq!(span.start_column, 1);
    }
}
