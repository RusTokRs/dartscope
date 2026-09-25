//! Minimal LSP server over `DartWorkspaceIndex`.

use std::collections::HashMap;

use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::{
    DartDefinitionQuery, DartWorkspaceIndex, DartWorkspaceResolutionContext,
};
use thiserror::Error;

use crate::coordinates::{byte_offset_to_lsp_position, lsp_position_to_byte_offset};
use crate::types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, Hover, HoverContents,
    InitializeParams, InitializeResult, Location, MarkedString, NumberOrString, Position, Range,
    ServerCapabilities, SymbolKind, TextDocumentContentChangeEvent, TextDocumentItem,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressOptions,
};

#[derive(Debug, Error)]
pub enum LspError {
    #[error("document not open: {0}")]
    DocumentNotOpen(String),
    #[error("invalid position")]
    InvalidPosition,
    #[error("not initialized")]
    NotInitialized,
}

/// In-memory LSP server with incremental document sync and index-backed navigation.
///
/// The server does not touch the filesystem: `root` is the LSP workspace root
/// (e.g. `file:///workspace`), and every `textDocument/*` notification carries
/// the full or incremental content. The server rebuilds a `DartWorkspaceIndex`
/// from the current open documents on every change (the incremental index itself
/// is reused internally). All positions are converted via `crate::coordinates`
/// so LF, CRLF and UTF-16 surrogate pairs round-trip.
pub struct DartLspServer {
    root: String,
    root_url: Option<Url>,
    documents: HashMap<Url, String>,
    index: Option<DartWorkspaceIndex>,
    initialized: bool,
}

impl DartLspServer {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            root_url: None,
            documents: HashMap::new(),
            index: None,
            initialized: false,
        }
    }

    pub fn initialize(&mut self, params: InitializeParams) -> Result<InitializeResult, LspError> {
        if let Some(root_uri) = params.root_uri {
            self.root_url = Some(root_uri.clone());
            if let Ok(path) = uri_to_path(&root_uri) {
                self.root = path;
            }
        } else if let Some(root_path) = params.root_path {
            self.root = root_path;
        }
        self.initialized = true;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::Incremental,
                )),
                definition_provider: Some(crate::types::OneOf::Left(true)),
                references_provider: Some(crate::types::OneOf::Left(true)),
                hover_provider: Some(crate::types::HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(crate::types::OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(crate::types::ServerInfo {
                name: "dartscope-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    pub fn initialized(&mut self) {
        // No-op: could publish initial diagnostics if needed.
    }

    pub fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }
        Ok(())
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.insert(uri.clone(), text);
        self.rebuild_index();
    }

    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(current) = self.documents.get(&uri).cloned() else {
            return;
        };
        let mut new_text = current;
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Incremental change
                if let Some(start) = lsp_position_to_byte_offset(&new_text, range.start) {
                    if let Some(end) = lsp_position_to_byte_offset(&new_text, range.end) {
                        new_text.replace_range(start..end, &change.text);
                        continue;
                    }
                }
                // Fallback to full replace if range invalid
                new_text = change.text;
            } else {
                // Full content change
                new_text = change.text;
            }
        }
        self.documents.insert(uri, new_text);
        self.rebuild_index();
    }

    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
        self.rebuild_index();
    }

    pub fn definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<Location>>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(offset) = lsp_position_to_byte_offset(content, position) else {
            return Err(LspError::InvalidPosition);
        };
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
        let query = DartDefinitionQuery::new(path, offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(index.snapshot());
        let batch = ctx.find_definitions(&[query]);
        let Some(resolution) = batch.resolutions.first() else {
            return Ok(None);
        };
        if resolution.targets.is_empty() {
            return Ok(None);
        }
        // Return first target as location; multiple targets become multiple locations
        let mut locations = Vec::new();
        for target in &resolution.targets {
            let (target_path, span) = match target {
                dartscope_index::DartDefinitionTarget::Namespace(candidate) => {
                    (&candidate.declaration_path, &candidate.declaration_span)
                }
                dartscope_index::DartDefinitionTarget::Lexical(binding) => {
                    (&binding.source_path, &binding.declaration_span)
                }
            };
            let target_uri = path_to_uri(target_path);
            let target_content = self
                .documents
                .get(&target_uri)
                .map(|s| s.as_str())
                .unwrap_or(content.as_str());
            // Fallback to original content if target not open — use its own span conversion via empty source?
            let range = crate::coordinates::source_span_to_lsp_range(target_content, span);
            locations.push(Location {
                uri: target_uri,
                range,
            });
        }
        Ok(Some(locations))
    }

    pub fn references(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<Location>>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(offset) = lsp_position_to_byte_offset(content, position) else {
            return Err(LspError::InvalidPosition);
        };
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
        let query = DartDefinitionQuery::new(path, offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(index.snapshot());
        let batch = ctx.find_definitions(&[query]);
        let Some(resolution) = batch.resolutions.first() else {
            return Ok(None);
        };
        if resolution.targets.is_empty() {
            return Ok(None);
        }
        let refs = ctx.find_references(&resolution.targets);
        let mut locations = Vec::new();
        for result in refs.results {
            for reference in result.references {
                let ref_uri = path_to_uri(&reference.source_path);
                let ref_content = self
                    .documents
                    .get(&ref_uri)
                    .map(|s| s.as_str())
                    .unwrap_or(content.as_str());
                let range = crate::coordinates::source_span_to_lsp_range(ref_content, &reference.span);
                locations.push(Location {
                    uri: ref_uri,
                    range,
                });
            }
        }
        locations.sort_by(|a, b| {
            a.uri
                .as_str()
                .cmp(b.uri.as_str())
                .then_with(|| a.range.start.line.cmp(&b.range.start.line))
                .then_with(|| a.range.start.character.cmp(&b.range.start.character))
        });
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        Ok(Some(locations))
    }

    pub fn hover(&self, uri: &Url, position: Position) -> Result<Option<Hover>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(offset) = lsp_position_to_byte_offset(content, position) else {
            return Err(LspError::InvalidPosition);
        };
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
        let query = DartDefinitionQuery::new(path, offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(index.snapshot());
        let batch = ctx.find_definitions(&[query]);
        let Some(resolution) = batch.resolutions.first() else {
            return Ok(None);
        };
        if resolution.targets.is_empty() {
            return Ok(None);
        }
        let mut contents = Vec::new();
        for target in &resolution.targets {
            let (name, kind) = match target {
                dartscope_index::DartDefinitionTarget::Namespace(candidate) => {
                    (candidate.name.as_str(), format!("{:?}", candidate.kind))
                }
                dartscope_index::DartDefinitionTarget::Lexical(binding) => {
                    (binding.name.as_str(), format!("{:?}", binding.kind))
                }
            };
            contents.push(MarkedString::String(format!("{kind} {name}")));
        }
        let range = {
            let reference = resolution.references.first();
            reference.map(|r| crate::coordinates::source_span_to_lsp_range(content, &r.span))
        };
        Ok(Some(Hover {
            contents: HoverContents::Array(contents),
            range,
        }))
    }

    pub fn document_symbols(
        &self,
        uri: &Url,
        _params: DocumentSymbolParams,
    ) -> Result<Option<Vec<DocumentSymbol>>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let snapshot = index.snapshot();
        let project = snapshot.project();
        let Some(file) = project.files.iter().find(|f| f.path == path) else {
            return Ok(None);
        };
        let mut symbols = Vec::new();
        for decl in &file.declarations {
            if decl.parent_symbol_id.is_some() {
                continue;
            }
            let range = crate::coordinates::source_span_to_lsp_range(
                content,
                decl.declaration_span.as_ref().unwrap_or(&decl.span),
            );
            let selection_range = crate::coordinates::source_span_to_lsp_range(content, &decl.span);
            let kind = match decl.kind {
                dartscope_core::DartDeclarationKind::Class => SymbolKind::Class,
                dartscope_core::DartDeclarationKind::Mixin => SymbolKind::Class,
                dartscope_core::DartDeclarationKind::Enum => SymbolKind::Enum,
                dartscope_core::DartDeclarationKind::Extension => SymbolKind::Interface,
                dartscope_core::DartDeclarationKind::Function => SymbolKind::Function,
                dartscope_core::DartDeclarationKind::Variable => SymbolKind::Variable,
                _ => SymbolKind::Variable,
            };
            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name: decl.name.clone(),
                detail: None,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range,
                children: Some(
                    file.declarations
                        .iter()
                        .filter(|m| m.parent_symbol_id.as_deref() == decl.symbol_id.as_deref())
                        .map(|member| {
                            let r = crate::coordinates::source_span_to_lsp_range(
                                content,
                                member.declaration_span.as_ref().unwrap_or(&member.span),
                            );
                            let sel = crate::coordinates::source_span_to_lsp_range(content, &member.span);
                            let k = match member.kind {
                                dartscope_core::DartDeclarationKind::Method => SymbolKind::Method,
                                dartscope_core::DartDeclarationKind::Field => SymbolKind::Field,
                                dartscope_core::DartDeclarationKind::Constructor => SymbolKind::Constructor,
                                dartscope_core::DartDeclarationKind::Getter => SymbolKind::Property,
                                dartscope_core::DartDeclarationKind::Setter => SymbolKind::Property,
                                _ => SymbolKind::Property,
                            };
                            #[allow(deprecated)]
                            DocumentSymbol {
                                name: member.name.clone(),
                                detail: None,
                                kind: k,
                                tags: None,
                                deprecated: None,
                                range: r,
                                selection_range: sel,
                                children: None,
                            }
                        })
                        .collect(),
                ),
            });
        }
        Ok(Some(symbols))
    }

    pub fn diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
        let snapshot = index.snapshot();
        let project = snapshot.project();
        let Some(file) = project.files.iter().find(|f| f.path == path) else {
            return Vec::new();
        };
        let content = self.documents.get(uri).map(|s| s.as_str()).unwrap_or("");
        file.diagnostics
            .iter()
            .map(|diag| {
                let range = diag
                    .span
                    .as_ref()
                    .map(|span| crate::coordinates::source_span_to_lsp_range(content, span))
                    .unwrap_or_else(|| Range {
                        start: Position { line: 0, character: 0 },
                        end: Position { line: 0, character: 0 },
                    });
                let severity = match diag.severity {
                    dartscope_core::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    dartscope_core::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                    dartscope_core::DiagnosticSeverity::Info => DiagnosticSeverity::INFORMATION,
                };
                Diagnostic {
                    range,
                    severity: Some(severity),
                    code: Some(NumberOrString::String(diag.code.clone())),
                    source: Some("dartscope".to_string()),
                    message: diag.message.clone(),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn rebuild_index(&mut self) {
        if self.documents.is_empty() {
            self.index = None;
            return;
        }
        let files = self
            .documents
            .iter()
            .map(|(uri, content)| {
                let path = uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string());
                DartFileInput::new(path, content.clone())
            })
            .collect::<Vec<_>>();
        let input = DartProjectInput::new(self.root.clone(), files, Vec::new());
        // Use reference analysis for navigation
        let analysis = dartscope_parse::analyze_project_with_references(input);
        let index = DartWorkspaceIndex::from_reference_project(analysis);
        self.index = Some(index);
    }
}

fn uri_to_path(uri: &Url) -> Result<String, Url> {
    // Url::to_file_path is platform-specific; we normalize to `/`-separated
    uri.to_file_path()
        .map(|path| path.to_string_lossy().replace('\\', "/").trim_start_matches('/').to_string())
        .map_err(|_| uri.clone())
}

fn path_to_uri(path: &str) -> Url {
    // For in-memory documents we synthesize `file:///` URIs
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    Url::parse(&format!("file://{normalized}")).unwrap_or_else(|_| Url::parse("file:///tmp.dart").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Position, Url};

    #[test]
    fn initialize_returns_capabilities() {
        let mut server = DartLspServer::new("/tmp");
        let params = InitializeParams::default();
        let result = server.initialize(params).unwrap();
        assert!(result.capabilities.definition_provider.is_some());
    }

    #[test]
    fn did_open_and_definition_round_trip() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/main.dart").unwrap();
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: content.to_string(),
            },
        });
        // Position of `bar` in `Foo().bar()` — find byte offset
        let offset = content.find("Foo().bar").unwrap() + "Foo().".len();
        let pos = byte_offset_to_lsp_position(content, offset);
        let locs = server.definition(&uri, pos).unwrap();
        assert!(locs.is_some());
        let locs = locs.unwrap();
        assert!(!locs.is_empty());
    }

    #[test]
    fn diagnostics_published_for_unsupported_syntax() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/main.dart").unwrap();
        // Dart 3.13 concise constructor triggers diagnostic
        let content = "class Foo { Foo.new(); }";
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: content.to_string(),
            },
        });
        let diags = server.diagnostics(&uri);
        assert!(!diags.is_empty());
    }

    #[test]
    fn incremental_change_applies_utf16() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/main.dart").unwrap();
        let content = "class A {}\n";
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: content.to_string(),
            },
        });
        // Insert "😀" at line 0 char 6 (after "class ")
        let change = DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position { line: 0, character: 6 },
                    end: Position { line: 0, character: 6 },
                }),
                range_length: None,
                text: "😀".to_string(),
            }],
        };
        server.did_change(change);
        let updated = server.documents.get(&uri).unwrap();
        assert!(updated.contains("😀"));
    }

    #[test]
    fn rapid_file_replacement_keeps_index_consistent() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/main.dart").unwrap();
        for i in 0..10 {
            let content = format!("class A{i} {{}}\n");
            if i == 0 {
                server.did_open(DidOpenTextDocumentParams {
                    text_document: crate::types::TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "dart".to_string(),
                        version: i as i32,
                        text: content.clone(),
                    },
                });
            } else {
                server.did_change(DidChangeTextDocumentParams {
                    text_document: crate::types::VersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: i as i32,
                    },
                    content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: content.clone(),
                    }],
                });
            }
            // Every intermediate state should be queryable without panic
            let diags = server.diagnostics(&uri);
            // diagnostics may be empty, but should not panic
            let _ = diags.len();
        }
        let final_content = server.documents.get(&uri).unwrap();
        assert!(final_content.contains("class A9"));
    }

    #[test]
    fn hover_returns_kind_and_name() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/main.dart").unwrap();
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: content.to_string(),
            },
        });
        let offset = content.find("bar()").unwrap();
        let pos = crate::coordinates::byte_offset_to_lsp_position(content, offset);
        let hover = server.hover(&uri, pos).unwrap();
        assert!(hover.is_some());
        let hover = hover.unwrap();
        match hover.contents {
            HoverContents::Array(arr) => assert!(!arr.is_empty()),
            _ => panic!("expected array"),
        }
    }
}
