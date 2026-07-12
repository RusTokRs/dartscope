use dartscope_core::DartDiagnostic;

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

pub(crate) fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut byte_start = 0usize;
    source
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

pub(crate) fn span_for_byte_range(
    source: &str,
    byte_start: usize,
    byte_end: usize,
) -> dartscope_core::SourceSpan {
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
    dartscope_core::SourceSpan {
        byte_start,
        byte_end,
        start_line: start.number,
        start_column: source[start.byte_start..byte_start].chars().count() + 1,
        end_line: end.number,
        end_column: source[end.byte_start..byte_end].chars().count() + 1,
    }
}

pub(crate) fn attach_diagnostic_paths(diagnostics: &mut [DartDiagnostic], path: &str) {
    for diagnostic in diagnostics {
        if diagnostic.path.is_none() {
            diagnostic.path = Some(path.to_string());
        }
    }
}
