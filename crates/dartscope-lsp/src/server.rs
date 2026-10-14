//! Minimal LSP server over `DartWorkspaceIndex`.

use std::collections::HashMap;

use dartscope_core::{
    DartDeclarationKind, DartFileInput, DartProjectInput, DartSymbolCandidate,
    DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionTarget, DartWorkspaceIndex, DartWorkspaceResolutionContext,
    DartWorkspaceSnapshot,
};
use thiserror::Error;

use crate::coordinates::{lsp_position_to_byte_offset, source_span_to_lsp_range};
use crate::types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentSymbol, Hover, HoverContents, InitializeParams, InitializeResult, LanguageString,
    Location, MarkedString, NumberOrString, Position, Range, ServerCapabilities, SymbolInformation,
    SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceSymbolParams,
};

/// Maximum number of entries returned by `workspace/symbol`.
const WORKSPACE_SYMBOL_LIMIT: usize = 100;

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
/// the full or incremental content. A `DartWorkspaceIndex` is maintained
/// incrementally: the first document builds it from all currently open content,
/// later sync events are applied as per-file upserts and removals so unaffected
/// documents keep their cached parse and resolution facts, and an empty document
/// set clears the index again. All positions are converted via
/// `crate::coordinates` so LF, CRLF and UTF-16 surrogate pairs round-trip.
///
/// Lint integration is explicit: this crate never depends on `dartscope-lints`.
/// A downstream adapter runs the configured lint analysis and feeds the results
/// back through [`DartLspServer::set_supplemental_diagnostics`], which merges
/// them into `diagnostics()` output without touching the parser facts.
pub struct DartLspServer {
    root: String,
    root_url: Option<Url>,
    documents: HashMap<Url, String>,
    document_versions: HashMap<Url, i32>,
    supplemental: HashMap<Url, Vec<Diagnostic>>,
    index: Option<DartWorkspaceIndex>,
    initialized: bool,
    shutdown: bool,
    #[cfg(feature = "lints")]
    lint_cache: Option<dartscope_lints::DartIncrementalLintCache>,
    #[cfg(feature = "lints")]
    last_workspace_update: Option<dartscope_index::DartWorkspaceUpdate>,
}

impl DartLspServer {
    pub fn new(root: impl Into<String>) -> Self {
        let mut server = Self {
            root: root.into(),
            root_url: None,
            documents: HashMap::new(),
            document_versions: HashMap::new(),
            supplemental: HashMap::new(),
            index: None,
            initialized: false,
            shutdown: false,
        };
        #[cfg(feature = "lints")]
        {
            server.lint_cache = None;
            server.last_workspace_update = None;
        }
        server
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
        self.shutdown = false;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::Incremental,
                )),
                definition_provider: Some(crate::types::OneOf::Left(true)),
                references_provider: Some(crate::types::OneOf::Left(true)),
                hover_provider: Some(crate::types::HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(crate::types::OneOf::Left(true)),
                workspace_symbol_provider: Some(crate::types::OneOf::Left(true)),
            },
            server_info: Some(crate::types::ServerInfo {
                name: "dartscope-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    pub fn initialized(&mut self) {
        // No-op: diagnostics are published by the stdio transport after each sync event.
    }

    pub fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }
        self.shutdown = true;
        Ok(())
    }

    /// `true` once `initialize` succeeded.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// `true` once `shutdown` succeeded; the stdio transport answers further
    /// requests with `InvalidRequest` until the client sends `exit`.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// The latest client-reported document `version`, if any sync event carried one.
    pub fn document_version(&self, uri: &Url) -> Option<i32> {
        self.document_versions.get(uri).copied()
    }

    /// The normalized workspace root path used for every index input.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// The `rootUri` reported by the client during `initialize`, if any.
    ///
    /// Adapters use this to translate synthetic document URIs back into the
    /// editor's view of the workspace; it is `None` when the client only sent
    /// `rootPath` or nothing at all.
    pub fn root_url(&self) -> Option<&Url> {
        self.root_url.as_ref()
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = params.text_document.text;
        self.document_versions.insert(uri.clone(), version);
        self.documents.insert(uri.clone(), text.clone());
        if self.index.is_some() {
            self.upsert_document(&uri, &text);
        } else {
            self.rebuild_index();
        }
    }

    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(current) = self.documents.get(&uri).cloned() else {
            return;
        };
        let mut new_text = current;
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Incremental change. Inverted or out-of-bounds ranges fall back
                // to the full content of the event, matching LSP client behavior.
                let valid = match (
                    lsp_position_to_byte_offset(&new_text, range.start),
                    lsp_position_to_byte_offset(&new_text, range.end),
                ) {
                    (Some(start), Some(end)) => Some(start..end),
                    _ => None,
                };
                if let Some(range_bytes) = valid.filter(|bytes| bytes.start <= bytes.end) {
                    new_text.replace_range(range_bytes, &change.text);
                    continue;
                }
                new_text = change.text;
            } else {
                // Full content change.
                new_text = change.text;
            }
        }
        if let Some(version) = params.text_document.version {
            self.document_versions.insert(uri.clone(), version);
        }
        self.documents.insert(uri.clone(), new_text.clone());
        if self.index.is_some() {
            self.upsert_document(&uri, &new_text);
        } else {
            self.rebuild_index();
        }
    }

    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = document_path(&uri);
        self.documents.remove(&uri);
        self.document_versions.remove(&uri);
        self.supplemental.remove(&uri);
        if self.documents.is_empty() {
            self.index = None;
        } else if let Some(index) = &mut self.index {
            let update = index.remove_file(&path);
            #[cfg(feature = "lints")]
            {
                self.last_workspace_update = Some(update);
            }
            #[cfg(not(feature = "lints"))]
            {
                let _ = update;
            }
        }
    }

    /// LSP `textDocument/didSave`: when the client registered `includeText`,
    /// the saved content replaces the document as a full change. Saves without
    /// text are accepted but cannot update content (the server never reads the
    /// filesystem).
    pub fn did_save(&mut self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(text) = params.text else {
            return;
        };
        if !self.documents.contains_key(&uri) {
            return;
        }
        self.documents.insert(uri.clone(), text.clone());
        if self.index.is_some() {
            self.upsert_document(&uri, &text);
        } else {
            self.rebuild_index();
        }
    }

    /// Replaces externally supplied diagnostics for one document, e.g. configured
    /// `dartscope-lints` findings produced by a downstream adapter. An empty slice
    /// removes the stored entries. Supplemental diagnostics never replace parser
    /// facts; `diagnostics()` returns parser diagnostics first and appends these
    /// entries in deterministic range order. They are cleared when the document
    /// closes.
    /// The workspace index maintained over the open documents, if any.
    ///
    /// Downstream lint adapters use this to run `dartscope-lints` over the same
    /// snapshot the navigation queries resolve against.
    pub fn index(&self) -> Option<&DartWorkspaceIndex> {
        self.index.as_ref()
    }

    /// The URIs of all currently open documents, sorted for determinism.
    pub fn open_document_uris(&self) -> Vec<Url> {
        let mut uris: Vec<Url> = self.documents.keys().cloned().collect();
        uris.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        uris
    }

    pub fn set_supplemental_diagnostics(&mut self, uri: &Url, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            self.supplemental.remove(uri);
        } else {
            self.supplemental.insert(uri.clone(), diagnostics);
        }
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
        let snapshot = index.snapshot();
        let path = document_path(uri);
        let query = DartDefinitionQuery::new(path.clone(), offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
        let batch = ctx.find_definitions(&[query]);
        let mut targets = batch
            .resolutions
            .first()
            .map(|resolution| resolution.targets.clone())
            .unwrap_or_default();
        if targets.is_empty() {
            // The query sits on a declaration identifier: declarations are not
            // reference facts, so resolve them directly onto their own spans.
            targets = declaration_targets_at(&snapshot, &path, offset);
        }
        if targets.is_empty() {
            return Ok(None);
        }
        let mut locations = Vec::new();
        for target in &targets {
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
            let range = source_span_to_lsp_range(target_content, span);
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
        self.references_at(uri, position, false)
    }

    /// LSP `textDocument/references`. When `include_declaration` is set the
    /// declaration span of every resolved target joins the result, matching the
    /// `ReferenceContext.includeDeclaration` semantics; querying from the
    /// declaration identifier itself resolves through the declaration fallback.
    pub fn references_at(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
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
        let snapshot = index.snapshot();
        let path = document_path(uri);
        let query = DartDefinitionQuery::new(path.clone(), offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
        let batch = ctx.find_definitions(&[query]);
        let mut targets = batch
            .resolutions
            .first()
            .map(|resolution| resolution.targets.clone())
            .unwrap_or_default();
        if targets.is_empty() {
            targets = declaration_targets_at(&snapshot, &path, offset);
        }
        if targets.is_empty() {
            return Ok(None);
        }
        let refs = ctx.find_references(&targets);
        let mut locations = Vec::new();
        for result in refs.results {
            for reference in result.references {
                let ref_uri = path_to_uri(&reference.source_path);
                let ref_content = self
                    .documents
                    .get(&ref_uri)
                    .map(|s| s.as_str())
                    .unwrap_or(content.as_str());
                let range = source_span_to_lsp_range(ref_content, &reference.span);
                locations.push(Location {
                    uri: ref_uri,
                    range,
                });
            }
        }
        if include_declaration {
            for target in &targets {
                let (target_path, span) = match target {
                    DartDefinitionTarget::Namespace(candidate) => {
                        (&candidate.declaration_path, &candidate.declaration_span)
                    }
                    DartDefinitionTarget::Lexical(binding) => {
                        (&binding.source_path, &binding.declaration_span)
                    }
                };
                let decl_uri = path_to_uri(target_path);
                let decl_content = self
                    .documents
                    .get(&decl_uri)
                    .map(String::as_str)
                    .unwrap_or(content.as_str());
                locations.push(Location {
                    uri: decl_uri,
                    range: source_span_to_lsp_range(decl_content, span),
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
        if locations.is_empty() {
            return Ok(None);
        }
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
        let snapshot = index.snapshot();
        let path = document_path(uri);
        let query = DartDefinitionQuery::new(path.clone(), offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
        let batch = ctx.find_definitions(&[query]);
        let mut targets = batch
            .resolutions
            .first()
            .map(|resolution| resolution.targets.clone())
            .unwrap_or_default();
        if targets.is_empty() {
            targets = declaration_targets_at(&snapshot, &path, offset);
        }
        if targets.is_empty() {
            return Ok(None);
        }
        let mut contents = Vec::new();
        for target in &targets {
            let (name, kind) = match target {
                dartscope_index::DartDefinitionTarget::Namespace(candidate) => (
                    candidate.name.as_str(),
                    declaration_kind_label(candidate.kind).to_string(),
                ),
                dartscope_index::DartDefinitionTarget::Lexical(binding) => {
                    (binding.name.as_str(), format!("{:?}", binding.kind))
                }
            };
            contents.push(MarkedString::LanguageString(LanguageString {
                language: "dart".to_string(),
                value: format!("{kind} {name}"),
            }));
        }
        let range = batch
            .resolutions
            .first()
            .and_then(|resolution| resolution.references.first())
            .map(|reference| source_span_to_lsp_range(content, &reference.span));
        Ok(Some(Hover {
            contents: HoverContents::Array(contents),
            range,
        }))
    }

    pub fn document_symbols(&self, uri: &Url) -> Result<Option<Vec<DocumentSymbol>>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let path = document_path(uri);
        let snapshot = index.snapshot();
        let project = snapshot.project();
        let Some(file) = project.files.iter().find(|file| file.path == path) else {
            return Ok(None);
        };
        let mut symbols = Vec::new();
        for decl in &file.declarations {
            if decl.parent_symbol_id.is_some() {
                continue;
            }
            let range = source_span_to_lsp_range(
                content,
                decl.declaration_span.as_ref().unwrap_or(&decl.span),
            );
            let selection_range = source_span_to_lsp_range(content, &decl.span);
            let children = file
                .declarations
                .iter()
                .filter(|member| member.parent_symbol_id.as_deref() == decl.symbol_id.as_deref())
                .map(|member| {
                    let r = source_span_to_lsp_range(
                        content,
                        member.declaration_span.as_ref().unwrap_or(&member.span),
                    );
                    let sel = source_span_to_lsp_range(content, &member.span);
                    let kind = if decl.kind == DartDeclarationKind::Enum {
                        SymbolKind::EnumMember
                    } else {
                        member_symbol_kind(member.kind)
                    };
                    DocumentSymbol {
                        name: member.name.clone(),
                        detail: Some(declaration_kind_label(member.kind).to_string()),
                        kind,
                        tags: None,
                        deprecated: None,
                        range: r,
                        selection_range: sel,
                        children: None,
                    }
                })
                .collect();
            symbols.push(DocumentSymbol {
                name: decl.name.clone(),
                detail: Some(declaration_kind_label(decl.kind).to_string()),
                kind: top_level_symbol_kind(decl.kind),
                tags: None,
                deprecated: None,
                range,
                selection_range,
                children: Some(children),
            });
        }
        Ok(Some(symbols))
    }

    /// LSP `workspace/symbol` over the open documents.
    ///
    /// Top-level declarations whose name contains the query (case-insensitive) are
    /// returned as flat `SymbolInformation` entries, sorted by name, then path, then
    /// range, and truncated to a bounded result size (100 entries) so the response
    /// stays small. An empty query returns the first bounded block in the same
    /// order.
    pub fn workspace_symbols(&self, params: &WorkspaceSymbolParams) -> Vec<SymbolInformation> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let snapshot = index.snapshot();
        let project = snapshot.project();
        let query = params.query.to_lowercase();
        let mut symbols: Vec<SymbolInformation> = Vec::new();
        for (uri, content) in &self.documents {
            let path = document_path(uri);
            let Some(file) = project.files.iter().find(|file| file.path == path) else {
                continue;
            };
            for decl in &file.declarations {
                if decl.parent_symbol_id.is_some() {
                    continue;
                }
                if !query.is_empty() && !decl.name.to_lowercase().contains(&query) {
                    continue;
                }
                let range = source_span_to_lsp_range(
                    content,
                    decl.declaration_span.as_ref().unwrap_or(&decl.span),
                );
                symbols.push(SymbolInformation {
                    name: decl.name.clone(),
                    kind: top_level_symbol_kind(decl.kind),
                    location: Location {
                        uri: uri.clone(),
                        range,
                    },
                    container_name: None,
                    tags: None,
                    deprecated: None,
                });
            }
        }
        symbols.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.location.uri.as_str().cmp(b.location.uri.as_str()))
                .then_with(|| {
                    a.location
                        .range
                        .start
                        .line
                        .cmp(&b.location.range.start.line)
                })
                .then_with(|| {
                    a.location
                        .range
                        .start
                        .character
                        .cmp(&b.location.range.start.character)
                })
        });
        symbols.truncate(WORKSPACE_SYMBOL_LIMIT);
        symbols
    }

    pub fn diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let parser = self
            .index
            .as_ref()
            .map(|index| index.snapshot())
            .map(|snapshot| {
                let path = document_path(uri);
                let content = self.documents.get(uri).map(String::as_str).unwrap_or("");
                let project = snapshot.project();
                project
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .into_iter()
                    .flat_map(|file| file.diagnostics.iter())
                    .map(|diag| {
                        let range = diag
                            .span
                            .as_ref()
                            .map(|span| source_span_to_lsp_range(content, span))
                            .unwrap_or_else(|| Range {
                                start: Position {
                                    line: 0,
                                    character: 0,
                                },
                                end: Position {
                                    line: 0,
                                    character: 0,
                                },
                            });
                        let severity = match diag.severity {
                            dartscope_core::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                            dartscope_core::DiagnosticSeverity::Warning => {
                                DiagnosticSeverity::WARNING
                            }
                            dartscope_core::DiagnosticSeverity::Info => {
                                DiagnosticSeverity::INFORMATION
                            }
                        };
                        Diagnostic {
                            related_information: None,
                            range,
                            severity: Some(severity),
                            code: Some(NumberOrString::String(diag.code.clone())),
                            source: Some("dartscope".to_string()),
                            message: diag.message.clone(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let Some(extra) = self.supplemental.get(uri) else {
            return parser;
        };
        let mut merged = parser;
        merged.extend(extra.iter().cloned());
        merged.sort_by(|a, b| {
            a.range
                .start
                .line
                .cmp(&b.range.start.line)
                .then_with(|| a.range.start.character.cmp(&b.range.start.character))
                .then_with(|| a.range.end.line.cmp(&b.range.end.line))
                .then_with(|| a.range.end.character.cmp(&b.range.end.character))
        });
        merged
    }

    /// Generation of the immutable index snapshot backing the current documents.
    ///
    /// The server rebuilds its index synchronously on every accepted sync event, so
    /// query results taken while this returns the same generation are mutually
    /// consistent. `None` means no documents are open. Consumers caching editor
    /// responses can compare generations to detect that a newer edit superseded
    /// their cached answer instead of assuming a background refresh.
    pub fn snapshot_generation(&self) -> Option<u64> {
        self.index
            .as_ref()
            .map(|index| index.snapshot().generation())
    }

    /// Feeds one document's new content through the index's per-file upsert so
    /// unaffected open documents keep their cached parse and resolution facts.
    fn upsert_document(&mut self, uri: &Url, text: &str) {
        if let Some(index) = &mut self.index {
            let analysis = dartscope_parse::analyze_file_with_references(DartFileInput::new(
                document_path(uri),
                text.to_string(),
            ));
            let update = index.upsert_file_with_references(analysis);
            #[cfg(feature = "lints")]
            {
                self.last_workspace_update = Some(update);
            }
            #[cfg(not(feature = "lints"))]
            {
                let _ = update;
            }
        }
    }

    fn rebuild_index(&mut self) {
        #[cfg(feature = "lints")]
        {
            self.lint_cache = None;
            self.last_workspace_update = None;
        }
        if self.documents.is_empty() {
            self.index = None;
            return;
        }
        let mut files = self
            .documents
            .iter()
            .map(|(uri, content)| DartFileInput::new(document_path(uri), content.clone()))
            .collect::<Vec<_>>();
        // `HashMap` iteration order is not stable; sort so the derived project and
        // every snapshot-backed product stay deterministic across runs.
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let input = DartProjectInput::new(self.root.clone(), files, Vec::new());
        let analysis = dartscope_parse::analyze_project_with_references(input);
        let index = DartWorkspaceIndex::from_reference_project(analysis);
        self.index = Some(index);
    }
}

#[cfg(feature = "lints")]
impl DartLspServer {
    /// Runs the configured `dartscope-lints` rules over the open-document
    /// workspace snapshot and feeds the findings through the supplemental
    /// diagnostics port, exactly as the downstream adapter contract expects.
    /// Documents without findings are explicitly cleared.
    pub fn refresh_lint_diagnostics(&mut self, config: &dartscope_lints::DartLintConfig) {
        let Self {
            index,
            documents,
            supplemental,
            lint_cache,
            last_workspace_update,
            ..
        } = self;
        let Some(index) = index.as_ref() else {
            *lint_cache = None;
            *last_workspace_update = None;
            supplemental.clear();
            return;
        };
        let snapshot = index.snapshot();
        let analysis = match (lint_cache, last_workspace_update.take()) {
            (cache @ None, _) => {
                *cache = Some(dartscope_lints::DartIncrementalLintCache::new(
                    &snapshot,
                    config.clone(),
                ));
                cache
                    .as_ref()
                    .expect("cache was just installed")
                    .analysis()
                    .clone()
            }
            (Some(cache), Some(update)) => {
                cache.update(&snapshot, &update, config);
                cache.analysis().clone()
            }
            (Some(cache), None) => cache.analysis().clone(),
        };
        let mut per_uri: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        for finding in analysis.diagnostics {
            let uri = path_to_uri(&finding.path);
            let content = documents.get(&uri).map(String::as_str).unwrap_or("");
            let range = finding
                .span
                .as_ref()
                .map(|span| source_span_to_lsp_range(content, span))
                .unwrap_or_else(|| Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                });
            let severity = match finding.severity {
                dartscope_core::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                dartscope_core::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                dartscope_core::DiagnosticSeverity::Info => DiagnosticSeverity::INFORMATION,
            };
            let related: Vec<DiagnosticRelatedInformation> = finding
                .related_paths
                .iter()
                .map(|related_path| DiagnosticRelatedInformation {
                    location: Location {
                        uri: path_to_uri(related_path),
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 0,
                            },
                        },
                    },
                    message: format!("related file `{related_path}`"),
                })
                .collect();
            per_uri.entry(uri).or_default().push(Diagnostic {
                related_information: (!related.is_empty()).then_some(related),
                range,
                severity: Some(severity),
                code: Some(NumberOrString::String(finding.rule_id.as_str().to_string())),
                source: Some("dartscope-lints".to_string()),
                message: finding.message,
            });
        }
        let uris: Vec<Url> = documents.keys().cloned().collect();
        for uri in uris {
            let entries = per_uri.remove(&uri).unwrap_or_default();
            if entries.is_empty() {
                supplemental.remove(&uri);
            } else {
                supplemental.insert(uri, entries);
            }
        }
    }
}

fn top_level_symbol_kind(kind: dartscope_core::DartDeclarationKind) -> SymbolKind {
    use dartscope_core::DartDeclarationKind as D;
    match kind {
        D::Class | D::Mixin | D::Typedef => SymbolKind::Class,
        D::Enum => SymbolKind::Enum,
        D::Extension | D::ExtensionType => SymbolKind::Interface,
        D::Function => SymbolKind::Function,
        D::Getter | D::Setter => SymbolKind::Property,
        _ => SymbolKind::Variable,
    }
}

fn member_symbol_kind(kind: dartscope_core::DartDeclarationKind) -> SymbolKind {
    use dartscope_core::DartDeclarationKind as D;
    match kind {
        D::Method => SymbolKind::Method,
        D::Field => SymbolKind::Field,
        D::Constructor => SymbolKind::Constructor,
        D::Operator => SymbolKind::Operator,
        _ => SymbolKind::Property,
    }
}

fn document_path(uri: &Url) -> String {
    uri_to_path(uri).unwrap_or_else(|_| uri.path().trim_start_matches('/').to_string())
}

fn uri_to_path(uri: &Url) -> Result<String, Url> {
    // Url::to_file_path is platform-specific; we normalize to `/`-separated
    // relative paths because the in-memory index keys files that way.
    let path = uri.to_file_path().ok_or_else(|| uri.clone())?;
    Ok(path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string())
}

fn path_to_uri(path: &str) -> Url {
    // For in-memory documents we synthesize `file:///` URIs.
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    Url::parse(&format!("file://{normalized}"))
        .unwrap_or_else(|_| Url::parse("file:///tmp.dart").expect("static url"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::byte_offset_to_lsp_position;
    use crate::types::{Position, Url};

    fn open_server(uri: &str, content: &str) -> (DartLspServer, Url) {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse(uri).unwrap();
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: content.to_string(),
            },
        });
        (server, uri)
    }

    #[test]
    fn initialize_returns_capabilities() {
        let mut server = DartLspServer::new("/tmp");
        let params = InitializeParams::default();
        let result = server.initialize(params).unwrap();
        assert!(result.capabilities.definition_provider.is_some());
        assert!(result.capabilities.workspace_symbol_provider.is_some());
    }

    #[test]
    fn did_open_and_definition_round_trip() {
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        let (server, uri) = open_server("file:///lib/main.dart", content);
        // Definition resolves unqualified reference facts; member accesses such as `.bar` stay
        // unresolved by design (v1 performs no receiver-type inference).
        let offset = content.find("Foo()").unwrap();
        let pos = byte_offset_to_lsp_position(content, offset);
        let locs = server.definition(&uri, pos).unwrap();
        assert!(locs.is_some());
        let locs = locs.unwrap();
        assert!(!locs.is_empty());
        assert_eq!(locs[0].uri.path(), "/lib/main.dart");
    }

    #[test]
    fn diagnostics_published_for_unsupported_syntax() {
        // The Dart 3.13 concise-constructor member form triggers a parser diagnostic.
        let (server, uri) = open_server("file:///lib/main.dart", "class Foo {\n  new();\n}\n");
        let diags = server.diagnostics(&uri);
        assert!(!diags.is_empty());
        assert!(diags.iter().any(|diag| diag.code
            == Some(NumberOrString::String(
                "unsupported_concise_constructor".to_string()
            ))));
    }

    #[test]
    fn supplemental_diagnostics_merge_after_parser_facts() {
        let (mut server, uri) = open_server("file:///lib/a.dart", "class A {\n  new();\n}\n");
        let parser_count = server.diagnostics(&uri).len();
        assert!(parser_count > 0);
        server.set_supplemental_diagnostics(
            &uri,
            vec![Diagnostic {
                related_information: None,
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 1,
                    },
                },
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(NumberOrString::String("dartscope.demo-rule".to_string())),
                source: Some("dartscope-lints".to_string()),
                message: "lint finding".to_string(),
            }],
        );
        let merged = server.diagnostics(&uri);
        assert_eq!(merged.len(), parser_count + 1);
        let has_lint = merged.iter().any(|diag| {
            diag.code == Some(NumberOrString::String("dartscope.demo-rule".to_string()))
        });
        assert!(has_lint);
        server.set_supplemental_diagnostics(&uri, Vec::new());
        assert_eq!(server.diagnostics(&uri).len(), parser_count);
    }

    #[test]
    fn workspace_symbols_match_case_insensitively_and_are_bounded() {
        let (server, uri) = open_server(
            "file:///lib/main.dart",
            "class Payment {}\nclass Customer {}\nclass Payroll {}\n",
        );
        let all = server.workspace_symbols(&WorkspaceSymbolParams {
            query: String::new(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
        assert_eq!(all.len(), 3);
        let matching = server.workspace_symbols(&WorkspaceSymbolParams {
            query: "pay".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
        let names = matching
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["Payment", "Payroll"]);
        assert!(matching.iter().all(|symbol| symbol.location.uri == uri));
    }

    #[test]
    fn snapshot_generation_tracks_document_updates() {
        let (mut server, uri) = open_server("file:///lib/main.dart", "class A {}\n");
        let first = server.snapshot_generation().expect("index after open");
        assert_eq!(server.snapshot_generation(), Some(first));
        server.did_change(DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class A {}\nclass B {}\n".to_string(),
            }],
        });
        assert!(server.snapshot_generation().is_some());
        server.did_close(DidCloseTextDocumentParams {
            text_document: crate::types::TextDocumentIdentifier { uri },
        });
        assert_eq!(server.snapshot_generation(), None);
    }

    #[test]
    fn incremental_change_applies_utf16() {
        let (mut server, uri) = open_server("file:///lib/main.dart", "class A {}\n");
        // Insert "😀" at line 0 char 6 (after "class ").
        let change = DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 6,
                    },
                    end: Position {
                        line: 0,
                        character: 6,
                    },
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
    fn inverted_change_range_falls_back_to_full_replace() {
        let (mut server, uri) = open_server("file:///lib/main.dart", "class A {}\n");
        let change = DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 7,
                    },
                    end: Position {
                        line: 0,
                        character: 1,
                    },
                }),
                range_length: None,
                text: "class B {}\n".to_string(),
            }],
        };
        server.did_change(change);
        let updated = server.documents.get(&uri).unwrap();
        assert_eq!(updated, "class B {}\n");
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
                        version: i,
                        text: content.clone(),
                    },
                });
            } else {
                server.did_change(DidChangeTextDocumentParams {
                    text_document: crate::types::VersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: Some(i),
                    },
                    content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: content.clone(),
                    }],
                });
            }
            // Every intermediate state must stay queryable without panic.
            let _ = server.diagnostics(&uri).len();
        }
        let final_content = server.documents.get(&uri).unwrap();
        assert!(final_content.contains("class A9"));
    }

    #[test]
    fn hover_returns_kind_and_name() {
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        let (server, uri) = open_server("file:///lib/main.dart", content);
        // Hover resolves through the same reference facts as `definition`,
        // so the query sits on the call-site occurrence of the resolvable `Foo`.
        let offset = content.find("Foo()").unwrap();
        let pos = byte_offset_to_lsp_position(content, offset);
        let hover = server.hover(&uri, pos).unwrap();
        assert!(hover.is_some());
        let hover = hover.unwrap();
        match &hover.contents {
            HoverContents::Array(arr) => {
                assert!(!arr.is_empty());
                let MarkedString::LanguageString(block) = &arr[0] else {
                    panic!("expected a dart code block");
                };
                assert_eq!(block.language, "dart");
                assert!(block.value.contains("Foo"));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn document_symbols_nest_members_under_their_type() {
        let content = "class Foo {\n  void bar() {}\n}\n";
        let (server, uri) = open_server("file:///lib/main.dart", content);
        let symbols = server.document_symbols(&uri).unwrap().unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
        let children = symbols[0].children.clone().unwrap_or_default();
        assert!(children.iter().any(|child| child.name == "bar"));
    }

    #[test]
    fn sync_events_track_document_version() {
        let (mut server, uri) = open_server("file:///lib/v.dart", "class V {}\n");
        assert_eq!(server.document_version(&uri), Some(1));
        server.did_change(DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: Some(5),
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class V { int m = 1; }\n".to_string(),
            }],
        });
        assert_eq!(server.document_version(&uri), Some(5));
        server.did_close(DidCloseTextDocumentParams {
            text_document: crate::types::TextDocumentIdentifier { uri: uri.clone() },
        });
        assert_eq!(server.document_version(&uri), None);
    }

    #[test]
    fn shutdown_flag_tracks_lifecycle() {
        let mut server = DartLspServer::new("/tmp");
        assert!(server.shutdown().is_err());
        server.initialize(InitializeParams::default()).unwrap();
        assert!(!server.is_shutdown());
        server.shutdown().unwrap();
        assert!(server.is_shutdown());
        server.initialize(InitializeParams::default()).unwrap();
        assert!(!server.is_shutdown());
    }

    #[test]
    fn closing_one_document_keeps_the_other_documents_resolvable() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let a_uri = Url::parse("file:///lib/a.dart").unwrap();
        let b_uri = Url::parse("file:///lib/b.dart").unwrap();
        let a_content = "import 'b.dart';\nvoid useB() { B(); }\n";
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: a_uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: a_content.to_string(),
            },
        });
        server.did_open(DidOpenTextDocumentParams {
            text_document: crate::types::TextDocumentItem {
                uri: b_uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: "class B {}\n".to_string(),
            },
        });
        let pos = byte_offset_to_lsp_position(a_content, a_content.find("B();").unwrap());
        let locs = server.definition(&a_uri, pos).unwrap();
        assert!(locs.is_some());
        assert_eq!(locs.unwrap()[0].uri.as_str(), "file:///lib/b.dart");
        server.did_close(DidCloseTextDocumentParams {
            text_document: crate::types::TextDocumentIdentifier { uri: b_uri.clone() },
        });
        // `a.dart` stays open and queryable; the removed target is now unresolved.
        let locs = server.definition(&a_uri, pos).unwrap();
        assert!(locs.is_none_or(|found| found.is_empty()));
        assert!(
            server
                .workspace_symbols(&WorkspaceSymbolParams {
                    query: "use".to_string(),
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                })
                .iter()
                .any(|symbol| symbol.name == "useB")
        );
    }

    #[test]
    fn did_change_without_version_still_updates_the_document() {
        let (mut server, uri) = open_server("file:///lib/v.dart", "class A {}\n");
        server.did_change(DidChangeTextDocumentParams {
            text_document: crate::types::VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: None,
            },
            content_changes: vec![crate::types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class A { int m = 1; }\n".to_string(),
            }],
        });
        let symbols = server.document_symbols(&uri).unwrap().unwrap();
        assert!(symbols.iter().any(|s| {
            s.children
                .as_ref()
                .is_some_and(|kids| kids.iter().any(|k| k.name == "m"))
        }));
        // The previously known version survives an unversioned change.
        assert_eq!(server.document_version(&uri), Some(1));
    }

    #[test]
    fn did_save_with_text_replaces_content() {
        let (mut server, uri) = open_server("file:///lib/s.dart", "class Old {}\n");
        server.did_save(DidSaveTextDocumentParams {
            text_document: crate::types::TextDocumentIdentifier { uri: uri.clone() },
            text: Some("class Saved {}\n".to_string()),
        });
        let symbols = server.document_symbols(&uri).unwrap().unwrap();
        assert!(symbols.iter().any(|s| s.name == "Saved"));
        assert!(!symbols.iter().any(|s| s.name == "Old"));
        // A text-less save cannot update content but never errors.
        server.did_save(DidSaveTextDocumentParams {
            text_document: crate::types::TextDocumentIdentifier { uri: uri.clone() },
            text: None,
        });
        let symbols = server.document_symbols(&uri).unwrap().unwrap();
        assert!(symbols.iter().any(|s| s.name == "Saved"));
    }

    #[test]
    fn declaration_identifier_resolves_to_itself() {
        let content = "class Beta { void run() {} }\nvoid main() { Beta(); }\n";
        let (server, uri) = open_server("file:///lib/d.dart", content);
        let decl_pos = Position {
            line: 0,
            character: 7,
        };
        let hover = server.hover(&uri, decl_pos).unwrap().unwrap();
        let HoverContents::Array(contents) = &hover.contents else {
            panic!("expected array");
        };
        let MarkedString::LanguageString(block) = &contents[0] else {
            panic!("expected code block");
        };
        assert_eq!(block.value, "class Beta");
        let locs = server.definition(&uri, decl_pos).unwrap().unwrap();
        assert_eq!(locs[0].uri.as_str(), "file:///lib/d.dart");
        assert_eq!(locs[0].range.start.line, 0);
        let with_decl = server.references_at(&uri, decl_pos, true).unwrap().unwrap();
        let without = server
            .references_at(&uri, decl_pos, false)
            .unwrap()
            .unwrap();
        assert!(
            with_decl
                .iter()
                .any(|l| l.range.start.line == 0 && l.range.start.character == 6)
        );
        assert!(without.iter().all(|l| l.range.start.line == 1));
        assert_eq!(with_decl.len(), without.len() + 1);
    }

    #[test]
    fn document_symbols_carry_detail_and_kind_refinements() {
        let content = "typedef Fn = void Function(int x);\nenum Color { red, green }\n";
        let (server, uri) = open_server("file:///lib/k.dart", content);
        let symbols = server.document_symbols(&uri).unwrap().unwrap();
        let typedef = symbols
            .iter()
            .find(|s| s.name == "Fn")
            .expect("typedef symbol");
        assert_eq!(typedef.detail.as_deref(), Some("typedef"));
        assert_eq!(typedef.kind, SymbolKind::Class);
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("enum symbol");
        assert_eq!(color.kind, SymbolKind::Enum);
        for kid in color.children.as_deref().unwrap_or(&[]) {
            assert_eq!(kid.kind, SymbolKind::EnumMember);
        }
    }
}
