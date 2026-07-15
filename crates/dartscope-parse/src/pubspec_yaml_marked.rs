use std::collections::BTreeSet;

use dartscope_core::{DartDiagnostic, SourceSpan};
use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, ScanError};

const ALIAS_CODE: &str = "pubspec_unsupported_yaml_alias";
const ALIAS_MESSAGE: &str =
    "YAML anchors, aliases, and merge keys are not supported by the pubspec parser";

pub(crate) fn parse_marked_yaml(source: &str) -> MarkedYamlDocument {
    let mut receiver = Receiver::new(source);
    let mut parser = Parser::new_from_str(source);
    if let Err(error) = parser.load(&mut receiver, true) {
        receiver.scan_error(error);
    }
    receiver.finish()
}

#[derive(Debug)]
pub(crate) struct MarkedYamlDocument {
    pub(crate) root: Option<Node>,
    pub(crate) diagnostics: Vec<DartDiagnostic>,
}

impl MarkedYamlDocument {
    pub(crate) fn into_diagnostics(self) -> Vec<DartDiagnostic> {
        self.diagnostics
    }
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) span: SourceSpan,
}

impl Node {
    fn scalar(value: String, span: SourceSpan) -> Self {
        Self {
            kind: NodeKind::Scalar(value),
            span,
        }
    }

    fn unsupported(span: SourceSpan) -> Self {
        Self {
            kind: NodeKind::Unsupported,
            span,
        }
    }
}

#[derive(Debug)]
pub(crate) enum NodeKind {
    Scalar(String),
    Sequence(Vec<Node>),
    Mapping(Vec<Entry>),
    Unsupported,
}

#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) key_span: SourceSpan,
    pub(crate) value: Node,
}

enum Frame {
    Sequence {
        start: Marker,
        items: Vec<Node>,
    },
    Mapping {
        start: Marker,
        entries: Vec<Entry>,
        pending_key: Option<Node>,
        seen_keys: BTreeSet<String>,
    },
}

struct Receiver<'a> {
    source: &'a str,
    byte_offsets: Vec<usize>,
    frames: Vec<Frame>,
    root: Option<Node>,
    diagnostics: Vec<DartDiagnostic>,
    documents: usize,
    accepting: bool,
}

impl<'a> Receiver<'a> {
    fn new(source: &'a str) -> Self {
        let mut byte_offsets = source
            .char_indices()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        byte_offsets.push(source.len());
        Self {
            source,
            byte_offsets,
            frames: Vec::new(),
            root: None,
            diagnostics: Vec::new(),
            documents: 0,
            accepting: false,
        }
    }

    fn byte_index(&self, mark: Marker) -> usize {
        self.byte_offsets
            .get(mark.index())
            .copied()
            .unwrap_or(self.source.len())
    }

    fn finish(self) -> MarkedYamlDocument {
        MarkedYamlDocument {
            root: self.root,
            diagnostics: self.diagnostics,
        }
    }

    fn begin_document(&mut self, mark: Marker) {
        self.documents += 1;
        self.accepting = self.documents == 1;
        if self.documents > 1 {
            self.diagnostics.push(DartDiagnostic::error(
                "pubspec_multiple_documents_unsupported",
                "pubspec.yaml must contain exactly one YAML document",
                Some(line_span(self.source, self.byte_index(mark), mark)),
            ));
        }
    }

    fn alias(&mut self, mark: Marker) {
        let span = line_span(self.source, self.byte_index(mark), mark);
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == ALIAS_CODE
                && diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|existing| existing.start_line == span.start_line)
        }) {
            return;
        }
        self.diagnostics.push(DartDiagnostic::warning(
            ALIAS_CODE,
            ALIAS_MESSAGE,
            Some(span),
        ));
    }

    fn anchor(&mut self, anchor: usize, mark: Marker) {
        if anchor != 0 {
            self.alias(mark);
        }
    }

    fn scan_error(&mut self, error: ScanError) {
        self.diagnostics.push(DartDiagnostic::error(
            "pubspec_invalid_yaml",
            format!("invalid pubspec YAML: {}", error.info()),
            Some(point_span(
                self.source,
                self.byte_index(*error.marker()),
                *error.marker(),
            )),
        ));
    }

    fn end_container(&mut self, mark: Marker, mapping: bool) {
        if !self.accepting {
            return;
        }
        let end_byte = self.byte_index(mark);
        let Some(frame) = self.frames.pop() else {
            return;
        };
        let node = match frame {
            Frame::Sequence { start, items } if !mapping => Node {
                kind: NodeKind::Sequence(items),
                span: range_span(self.byte_index(start), start, end_byte, mark),
            },
            Frame::Mapping { start, entries, .. } if mapping => Node {
                kind: NodeKind::Mapping(entries),
                span: range_span(self.byte_index(start), start, end_byte, mark),
            },
            other => {
                self.frames.push(other);
                return;
            }
        };
        self.push_node(node);
    }

    fn push_node(&mut self, mut node: Node) {
        let mut new_diagnostics = Vec::new();
        if let Some(frame) = self.frames.last_mut() {
            match frame {
                Frame::Sequence { items, .. } => items.push(node),
                Frame::Mapping {
                    entries,
                    pending_key,
                    seen_keys,
                    ..
                } => {
                    if pending_key.is_none() {
                        if let NodeKind::Scalar(key) = &node.kind {
                            node.span = mapping_key_span(self.source, &node.span);
                            if key == "<<" {
                                new_diagnostics.push(DartDiagnostic::warning(
                                    ALIAS_CODE,
                                    ALIAS_MESSAGE,
                                    Some(node.span.clone()),
                                ));
                            }
                            if !seen_keys.insert(key.clone()) {
                                new_diagnostics.push(DartDiagnostic::error(
                                    "pubspec_duplicate_key",
                                    format!("duplicate YAML mapping key: {key}"),
                                    Some(node.span.clone()),
                                ));
                            }
                        } else {
                            new_diagnostics.push(DartDiagnostic::error(
                                "pubspec_invalid_yaml",
                                "pubspec YAML mapping keys must be scalar values",
                                Some(node.span.clone()),
                            ));
                        }
                        *pending_key = Some(node);
                    } else {
                        let key_node = pending_key.take().expect("mapping key must exist");
                        if let NodeKind::Scalar(key) = key_node.kind {
                            entries.push(Entry {
                                key,
                                key_span: key_node.span,
                                value: node,
                            });
                        }
                    }
                }
            }
        } else if self.root.is_none() {
            self.root = Some(node);
        }
        for diagnostic in new_diagnostics {
            if !same_diagnostic_line(&self.diagnostics, &diagnostic) {
                self.diagnostics.push(diagnostic);
            }
        }
    }
}

impl MarkedEventReceiver for Receiver<'_> {
    fn on_event(&mut self, event: Event, mark: Marker) {
        match event {
            Event::Nothing | Event::StreamStart | Event::StreamEnd => {}
            Event::DocumentStart => self.begin_document(mark),
            Event::DocumentEnd => self.accepting = false,
            Event::Alias(_) if self.accepting => {
                self.alias(mark);
                let byte_start = self.byte_index(mark);
                self.push_node(Node::unsupported(point_span(self.source, byte_start, mark)));
            }
            Event::Alias(_) => {}
            Event::Scalar(value, _, anchor, _) if self.accepting => {
                self.anchor(anchor, mark);
                let span = scalar_span(self.byte_index(mark), mark, &value);
                self.push_node(Node::scalar(value, span));
            }
            Event::Scalar(_, _, _, _) => {}
            Event::SequenceStart(anchor, _) if self.accepting => {
                self.anchor(anchor, mark);
                self.frames.push(Frame::Sequence {
                    start: mark,
                    items: Vec::new(),
                });
            }
            Event::SequenceStart(_, _) => {}
            Event::SequenceEnd => self.end_container(mark, false),
            Event::MappingStart(anchor, _) if self.accepting => {
                self.anchor(anchor, mark);
                self.frames.push(Frame::Mapping {
                    start: mark,
                    entries: Vec::new(),
                    pending_key: None,
                    seen_keys: BTreeSet::new(),
                });
            }
            Event::MappingStart(_, _) => {}
            Event::MappingEnd => self.end_container(mark, true),
        }
    }
}

fn same_diagnostic_line(existing: &[DartDiagnostic], candidate: &DartDiagnostic) -> bool {
    existing.iter().any(|diagnostic| {
        diagnostic.code == candidate.code
            && diagnostic.span.as_ref().is_some_and(|existing_span| {
                candidate
                    .span
                    .as_ref()
                    .is_some_and(|span| existing_span.start_line == span.start_line)
            })
    })
}

fn scalar_span(byte_start: usize, mark: Marker, value: &str) -> SourceSpan {
    SourceSpan {
        byte_start,
        byte_end: byte_start + value.len(),
        start_line: mark.line(),
        start_column: mark.col(),
        end_line: mark.line(),
        end_column: mark.col() + value.chars().count(),
    }
}

fn point_span(source: &str, byte_start: usize, mark: Marker) -> SourceSpan {
    let byte_start = byte_start.min(source.len());
    let byte_end = source[byte_start..]
        .chars()
        .next()
        .map_or(byte_start, |ch| byte_start + ch.len_utf8());
    SourceSpan {
        byte_start,
        byte_end,
        start_line: mark.line(),
        start_column: mark.col(),
        end_line: mark.line(),
        end_column: mark.col() + if byte_end > byte_start { 1 } else { 0 },
    }
}

fn range_span(byte_start: usize, start: Marker, byte_end: usize, end: Marker) -> SourceSpan {
    SourceSpan {
        byte_start,
        byte_end,
        start_line: start.line(),
        start_column: start.col(),
        end_line: end.line(),
        end_column: end.col(),
    }
}

fn line_span(source: &str, marker: usize, mark: Marker) -> SourceSpan {
    let marker = marker.min(source.len());
    let byte_start = source[..marker].rfind('\n').map_or(0, |index| index + 1);
    let byte_end = source[marker..]
        .find('\n')
        .map_or(source.len(), |index| marker + index);
    let line = &source[byte_start..byte_end];
    SourceSpan::line(
        mark.line(),
        byte_start,
        line.strip_suffix('\r').unwrap_or(line),
    )
}

fn mapping_key_span(source: &str, scalar: &SourceSpan) -> SourceSpan {
    let start = scalar.byte_start.min(source.len());
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |index| start + index);
    let line = &source[start..line_end];
    let line = line.strip_suffix('\r').unwrap_or(line);
    let raw_key = find_mapping_colon(line)
        .map_or(line, |index| &line[..index])
        .trim_end();
    SourceSpan {
        byte_start: start,
        byte_end: start + raw_key.len(),
        start_line: scalar.start_line,
        start_column: scalar.start_column,
        end_line: scalar.start_line,
        end_column: scalar.start_column + raw_key.chars().count(),
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
            ':' => return Some(index),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nested_marked_tree() {
        let document = parse_marked_yaml(concat!(
            "name: demo\n",
            "environment:\n",
            "  sdk: ^3.4.0\n",
            "flutter:\n",
            "  assets:\n",
            "    - assets/logo.svg\n",
        ));

        assert!(document.diagnostics.is_empty());
        let root = document.root.expect("root mapping");
        let NodeKind::Mapping(entries) = root.kind else {
            panic!("expected root mapping");
        };
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, "name");
        assert!(entries[0].key_span.byte_end > entries[0].key_span.byte_start);
        assert!(matches!(&entries[2].value.kind, NodeKind::Mapping(_)));
    }

    #[test]
    fn preserves_crlf_and_unicode_scalar_offsets() {
        let source = concat!(
            "name: demo\r\n",
            "description: Привет\r\n",
            "flutter:\r\n",
            "  assets:\r\n",
            "    - assets/иконка.png\r\n",
        );
        let document = parse_marked_yaml(source);
        let expected = source.find("assets/иконка.png").expect("asset scalar");

        assert!(document.diagnostics.is_empty());
        assert!(contains_scalar_at(
            document.root.as_ref(),
            "assets/иконка.png",
            expected
        ));
    }

    #[test]
    fn reports_duplicate_keys_and_additional_documents() {
        let document = parse_marked_yaml(concat!(
            "name: first\n",
            "name: second\n",
            "---\n",
            "name: ignored\n",
        ));

        assert!(document.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_duplicate_key"
                && diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| span.start_line == 2)
        }));
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "pubspec_multiple_documents_unsupported" })
        );
    }

    #[test]
    fn reports_anchors_aliases_and_merge_keys() {
        let document = parse_marked_yaml(concat!(
            "defaults: &defaults\n",
            "  path: ../local\n",
            "dependency: *defaults\n",
            "merged:\n",
            "  <<: *defaults\n",
        ));
        let warnings = document
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == ALIAS_CODE)
            .count();

        assert_eq!(warnings, 3);
    }

    #[test]
    fn converts_scanner_failures_to_stable_diagnostics() {
        let diagnostics = parse_marked_yaml("flutter: [unterminated\n").into_diagnostics();

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "pubspec_invalid_yaml")
        );
    }

    fn contains_scalar_at(node: Option<&Node>, value: &str, byte_start: usize) -> bool {
        let Some(node) = node else {
            return false;
        };
        match &node.kind {
            NodeKind::Scalar(actual) => actual == value && node.span.byte_start == byte_start,
            NodeKind::Sequence(items) => items
                .iter()
                .any(|item| contains_scalar_at(Some(item), value, byte_start)),
            NodeKind::Mapping(entries) => entries
                .iter()
                .any(|entry| contains_scalar_at(Some(&entry.value), value, byte_start)),
            NodeKind::Unsupported => false,
        }
    }
}
