use std::collections::BTreeSet;

use dartscope_core::{DartDiagnostic, SourceSpan};

use crate::pubspec_yaml_subset::{
    leading_indentation_contains_tab, leading_space_count, strip_yaml_comment, yaml_key_value,
};
use crate::source_lines::{SourceLine, source_lines};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DuplicatePubspecKey {
    pub(crate) key: String,
    pub(crate) span: SourceSpan,
}

#[derive(Debug, Default)]
pub(crate) struct PubspecSyntaxCheck {
    bare_wildcard_lines: Vec<usize>,
    invalid_flow_spans: Vec<SourceSpan>,
    invalid_indentation_spans: Vec<SourceSpan>,
    unsupported_alias_spans: Vec<SourceSpan>,
    duplicate_keys: Vec<DuplicatePubspecKey>,
    multiple_document_spans: Vec<SourceSpan>,
}

impl PubspecSyntaxCheck {
    pub(crate) fn is_bare_wildcard_line(&self, line: usize) -> bool {
        self.bare_wildcard_lines.contains(&line)
    }

    pub(crate) fn invalid_flow_spans(&self) -> &[SourceSpan] {
        &self.invalid_flow_spans
    }

    pub(crate) fn invalid_indentation_spans(&self) -> &[SourceSpan] {
        &self.invalid_indentation_spans
    }

    pub(crate) fn unsupported_alias_spans(&self) -> &[SourceSpan] {
        &self.unsupported_alias_spans
    }

    pub(crate) fn duplicate_keys(&self) -> &[DuplicatePubspecKey] {
        &self.duplicate_keys
    }

    pub(crate) fn multiple_document_spans(&self) -> &[SourceSpan] {
        &self.multiple_document_spans
    }
}

pub(crate) struct PreparedPubspecSource {
    pub(crate) source: String,
    pub(crate) syntax: PubspecSyntaxCheck,
}

pub(crate) fn prepare_pubspec_source(source: &str) -> PreparedPubspecSource {
    let mut scanner = SyntaxScanner::default();
    let mut sanitized = source.as_bytes().to_vec();
    let mut stop_at = None;

    for source_line in source_lines(source) {
        if scanner.observe_document_boundary(source_line) {
            blank_line(&mut sanitized, source_line);
            continue;
        }
        if scanner.should_stop() {
            stop_at = Some(source_line.byte_start);
            break;
        }
        scanner.observe_content(source_line);
    }

    if let Some(byte_start) = stop_at {
        blank_range(&mut sanitized, byte_start, source.len());
    }

    PreparedPubspecSource {
        source: String::from_utf8(sanitized).expect("blanking source bytes preserves UTF-8"),
        syntax: scanner.finish(),
    }
}

pub(crate) fn append_common_syntax_diagnostics(
    diagnostics: &mut Vec<DartDiagnostic>,
    path: &str,
    syntax: &PubspecSyntaxCheck,
) {
    for span in syntax.invalid_indentation_spans() {
        push_unique_diagnostic(
            diagnostics,
            DartDiagnostic::error(
                "pubspec_invalid_indentation",
                "pubspec.yaml indentation must use spaces, not tabs",
                Some(span.clone()),
            )
            .with_path(path),
        );
    }
    for span in syntax.unsupported_alias_spans() {
        push_unique_diagnostic(
            diagnostics,
            DartDiagnostic::warning(
                "pubspec_unsupported_yaml_alias",
                "YAML anchors, aliases, and merge keys are not supported by the pubspec parser",
                Some(span.clone()),
            )
            .with_path(path),
        );
    }
    for duplicate in syntax.duplicate_keys() {
        push_unique_diagnostic(
            diagnostics,
            DartDiagnostic::error(
                "pubspec_duplicate_key",
                format!("duplicate YAML mapping key: {}", duplicate.key),
                Some(duplicate.span.clone()),
            )
            .with_path(path),
        );
    }
    for span in syntax.multiple_document_spans() {
        push_unique_diagnostic(
            diagnostics,
            DartDiagnostic::error(
                "pubspec_multiple_documents_unsupported",
                "pubspec.yaml must contain exactly one YAML document",
                Some(span.clone()),
            )
            .with_path(path),
        );
    }
}

fn push_unique_diagnostic(diagnostics: &mut Vec<DartDiagnostic>, candidate: DartDiagnostic) {
    let duplicate = diagnostics.iter().any(|diagnostic| {
        diagnostic.code == candidate.code
            && diagnostic.span.as_ref().is_some_and(|existing| {
                candidate
                    .span
                    .as_ref()
                    .is_some_and(|span| existing.start_line == span.start_line)
            })
    });
    if !duplicate {
        diagnostics.push(candidate);
    }
}

#[derive(Default)]
struct SyntaxScanner {
    syntax: PubspecSyntaxCheck,
    saw_leading_document_start: bool,
    saw_content: bool,
    document_closed: bool,
    stop: bool,
    top_level_keys: BTreeSet<String>,
    direct_mapping_keys: BTreeSet<String>,
    direct_mapping_indent: Option<usize>,
    in_direct_mapping: bool,
    in_dependency_section: bool,
    dependency_direct_indent: Option<usize>,
}

impl SyntaxScanner {
    fn observe_document_boundary(&mut self, source_line: SourceLine<'_>) -> bool {
        if leading_indentation_contains_tab(source_line.text) {
            return false;
        }
        let trimmed = strip_yaml_comment(source_line.text).trim();
        if trimmed.is_empty() || leading_space_count(source_line.text) != 0 {
            return false;
        }

        match trimmed {
            "---"
                if !self.saw_content
                    && !self.document_closed
                    && !self.saw_leading_document_start =>
            {
                self.saw_leading_document_start = true;
                true
            }
            "---" => {
                self.reject_additional_document(source_line);
                false
            }
            "..." if !self.document_closed => {
                self.document_closed = true;
                true
            }
            "..." => {
                self.reject_additional_document(source_line);
                false
            }
            _ if self.document_closed => {
                self.reject_additional_document(source_line);
                false
            }
            _ => false,
        }
    }

    fn observe_content(&mut self, source_line: SourceLine<'_>) {
        if self.stop {
            return;
        }
        if leading_indentation_contains_tab(source_line.text) {
            self.syntax.invalid_indentation_spans.push(SourceSpan::line(
                source_line.number,
                source_line.byte_start,
                source_line.text,
            ));
            return;
        }

        let yaml = strip_yaml_comment(source_line.text);
        let trimmed = yaml.trim();
        if trimmed.is_empty() {
            return;
        }
        self.saw_content = true;

        let indent = leading_space_count(source_line.text);
        let span = SourceSpan::line(source_line.number, source_line.byte_start, source_line.text);
        if contains_unsupported_alias_syntax(trimmed) {
            self.syntax.unsupported_alias_spans.push(span.clone());
        }
        if indent == 0 {
            self.observe_top_level(trimmed, span);
            return;
        }

        self.observe_direct_mapping_key(trimmed, indent, &span);
        self.observe_dependency_syntax(trimmed, indent, span);
    }

    fn observe_top_level(&mut self, trimmed: &str, span: SourceSpan) {
        self.in_direct_mapping = false;
        self.in_dependency_section = false;
        self.direct_mapping_indent = None;
        self.dependency_direct_indent = None;
        self.direct_mapping_keys.clear();

        let Some((key, value)) = yaml_key_value(trimmed) else {
            return;
        };
        self.record_key(key, 0, trimmed, span, true);

        self.in_direct_mapping = value.is_none()
            && matches!(
                key,
                "dependencies"
                    | "dev_dependencies"
                    | "dependency_overrides"
                    | "environment"
                    | "flutter"
            );
        self.in_dependency_section = value.is_none()
            && matches!(
                key,
                "dependencies" | "dev_dependencies" | "dependency_overrides"
            );
    }

    fn observe_direct_mapping_key(&mut self, trimmed: &str, indent: usize, span: &SourceSpan) {
        if !self.in_direct_mapping {
            return;
        }
        let expected_indent = *self.direct_mapping_indent.get_or_insert(indent);
        if indent < expected_indent {
            self.in_direct_mapping = false;
            return;
        }
        if indent != expected_indent || trimmed.starts_with('-') {
            return;
        }
        let Some((key, _)) = yaml_key_value(trimmed) else {
            return;
        };
        self.record_key(key, indent, trimmed, span.clone(), false);
    }

    fn observe_dependency_syntax(&mut self, trimmed: &str, indent: usize, span: SourceSpan) {
        if !self.in_dependency_section {
            return;
        }
        let expected_indent = *self.dependency_direct_indent.get_or_insert(indent);
        if indent < expected_indent {
            self.in_dependency_section = false;
            return;
        }
        if indent != expected_indent {
            return;
        }

        let Some((_, value)) = yaml_key_value(trimmed) else {
            return;
        };
        let Some(value) = value else {
            return;
        };
        if value == "*" {
            self.syntax.bare_wildcard_lines.push(span.start_line);
        }
        if value.starts_with('{') && !flow_delimiters_are_balanced(value) {
            self.syntax.invalid_flow_spans.push(span);
        }
    }

    fn record_key(
        &mut self,
        key: &str,
        indent: usize,
        trimmed: &str,
        line_span: SourceSpan,
        top_level: bool,
    ) {
        let inserted = if top_level {
            self.top_level_keys.insert(key.to_string())
        } else {
            self.direct_mapping_keys.insert(key.to_string())
        };
        if !inserted {
            self.syntax.duplicate_keys.push(DuplicatePubspecKey {
                key: key.to_string(),
                span: mapping_key_span(&line_span, indent, trimmed),
            });
        }
    }

    fn reject_additional_document(&mut self, source_line: SourceLine<'_>) {
        self.syntax.multiple_document_spans.push(SourceSpan::line(
            source_line.number,
            source_line.byte_start,
            source_line.text,
        ));
        self.stop = true;
    }

    fn should_stop(&self) -> bool {
        self.stop
    }

    fn finish(self) -> PubspecSyntaxCheck {
        self.syntax
    }
}

fn contains_unsupported_alias_syntax(value: &str) -> bool {
    if value.starts_with("<<:") {
        return true;
    }
    if yaml_key_value(value).is_some_and(|(_, scalar)| scalar == Some("*")) {
        return false;
    }

    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;
    for ch in value.chars() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '&' | '*'
                    if previous.is_none_or(|previous: char| {
                        previous.is_whitespace() || previous == ':'
                    }) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        previous = Some(ch);
    }
    false
}

fn blank_line(bytes: &mut [u8], source_line: SourceLine<'_>) {
    blank_range(
        bytes,
        source_line.byte_start,
        source_line.byte_start + source_line.text.len(),
    );
}

fn blank_range(bytes: &mut [u8], byte_start: usize, byte_end: usize) {
    for byte in &mut bytes[byte_start..byte_end] {
        if !matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
}

fn mapping_key_span(line_span: &SourceSpan, indent: usize, trimmed: &str) -> SourceSpan {
    let key_end = find_mapping_colon(trimmed).unwrap_or(trimmed.len());
    let raw_key = trimmed[..key_end].trim_end();
    SourceSpan {
        byte_start: line_span.byte_start + indent,
        byte_end: line_span.byte_start + indent + raw_key.len(),
        start_line: line_span.start_line,
        start_column: indent + 1,
        end_line: line_span.start_line,
        end_column: indent + raw_key.chars().count() + 1,
    }
}

fn find_mapping_colon(value: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut chars = value.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ':' => {
                let is_separator = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
                if is_separator {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn flow_delimiters_are_balanced(value: &str) -> bool {
    let mut delimiters = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut chars = value.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' | '[' => delimiters.push(ch),
            '}' => {
                if delimiters.pop() != Some('{') {
                    return false;
                }
                if delimiters.is_empty() && !value[index + ch.len_utf8()..].trim().is_empty() {
                    return false;
                }
            }
            ']' if delimiters.pop() != Some('[') => return false,
            ']' => {}
            _ => {}
        }
    }

    quote.is_none() && !escaped && delimiters.is_empty()
}
