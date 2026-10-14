//! Minimal LSP server over `DartWorkspaceIndex`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartFileInput, DartIdentifierReferenceKind,
    DartLexicalBindingKind, DartProjectInput, DartSymbolCandidate, DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionTarget, DartWorkspaceIndex, DartWorkspaceResolutionContext,
    DartWorkspaceSnapshot,
};
use thiserror::Error;

use crate::coordinates::{lsp_position_to_byte_offset, source_span_to_lsp_range};
use crate::types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbol, Hover, HoverContents,
    InitializeParams, InitializeResult, LanguageString, Location, MarkedString, NumberOrString,
    Position, PrepareRenameResult, Range, RenameOptions, SemanticTokens, SemanticTokensLegend,
    SemanticTokensOptions, ServerCapabilities, SymbolInformation, SymbolKind,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url, WorkspaceEdit,
    WorkspaceSymbolParams,
};

#[cfg(feature = "lints")]
use crate::types::DiagnosticRelatedInformation;

/// Byte range of a declaration's own identifier (the name occurrence at the
/// start of its declaration span). Returns `None` when the current buffer no
/// longer contains the name where the parser recorded it.
fn declaration_name_range(content: &str, decl: &DartDeclaration) -> Option<(usize, usize)> {
    let head = decl.declaration_span.as_ref().unwrap_or(&decl.span);
    let text = content.get(head.byte_start..head.byte_end)?;
    let rel = text.find(decl.name.as_str())?;
    let start = head.byte_start + rel;
    Some((start, start + decl.name.len()))
}

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
    #[error("invalid rename name: {0}")]
    InvalidName(String),
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
        Self {
            root: root.into(),
            root_url: None,
            documents: HashMap::new(),
            document_versions: HashMap::new(),
            supplemental: HashMap::new(),
            index: None,
            initialized: false,
            shutdown: false,
            #[cfg(feature = "lints")]
            lint_cache: None,
            #[cfg(feature = "lints")]
            last_workspace_update: None,
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
                semantic_tokens_provider: Some(SemanticTokensOptions {
                    legend: SemanticTokensLegend {
                        token_types: SEMANTIC_TOKEN_TYPES
                            .iter()
                            .map(|token_type| (*token_type).to_string())
                            .collect(),
                        token_modifiers: SEMANTIC_TOKEN_MODIFIERS
                            .iter()
                            .map(|modifier| (*modifier).to_string())
                            .collect(),
                    },
                    range: Some(false),
                    full: true,
                    work_done_progress_options: None,
                }),
                rename_provider: Some(crate::types::OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: None,
                })),
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
        let (targets, _) = self.resolve_targets(&snapshot, &path, content, offset);
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
        let (targets, _) = self.resolve_targets(&snapshot, &path, content, offset);
        if targets.is_empty() {
            return Ok(None);
        }
        let ctx = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
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

    /// Resolve the definition targets for a query position: index resolution
    /// first, then the declaration-name fallback. Shared by `definition`,
    /// `references`, `hover`, and the rename entry points so all of them agree
    /// on which symbol the position points at.
    fn resolve_targets(
        &self,
        snapshot: &DartWorkspaceSnapshot,
        path: &str,
        content: &str,
        offset: usize,
    ) -> (Vec<DartDefinitionTarget>, Option<Range>) {
        let query = DartDefinitionQuery::new(path.to_string(), offset);
        let ctx = DartWorkspaceResolutionContext::from_snapshot(snapshot);
        let batch = ctx.find_definitions(&[query]);
        let mut targets = batch
            .resolutions
            .first()
            .map(|resolution| resolution.targets.clone())
            .unwrap_or_default();
        // The exact occurrence range for hover, when the query hit a
        // reference fact (the fallback paths have none and stay range-less).
        let occurrence = batch
            .resolutions
            .first()
            .and_then(|resolution| resolution.references.first())
            .map(|reference| source_span_to_lsp_range(content, &reference.span));
        if targets.is_empty() {
            targets = declaration_targets_at(snapshot, path, offset, content);
        }
        if targets.is_empty() {
            // Receiver-type-aware fallback for qualified member accesses whose
            // receiver type is lexically visible in the same document.
            targets = receiver_type_targets_at(snapshot, path, content, offset);
        }
        (targets, occurrence)
    }

    /// LSP `textDocument/semanticTokens/full`: identifier highlights derived
    /// purely from index facts (declaration names, reference facts, and
    /// lexical bindings). No syntax-tree access; a token is emitted only
    /// where a recorded span resolves inside the current buffer.
    pub fn semantic_tokens(&self, uri: &Url) -> Result<Option<SemanticTokens>, LspError> {
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let snapshot = index.snapshot();
        let path = document_path(uri);
        let mut tokens: BTreeMap<(u32, u32), (u32, u32, u32)> = BTreeMap::new();
        let declarations = snapshot
            .project()
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| &file.declarations);
        let mut kind_by_symbol: BTreeMap<&str, DartDeclarationKind> = BTreeMap::new();
        if let Some(declarations) = declarations {
            for decl in declarations {
                if let Some(symbol_id) = &decl.symbol_id {
                    kind_by_symbol.insert(symbol_id, decl.kind);
                }
            }
        }
        // Declarations carry the `declaration` modifier (and `static` when the
        // recorded header text contains the keyword); enum members refine to
        // the `enumMember` token type.
        if let Some(declarations) = declarations {
            for decl in declarations {
                let Some((start, end)) = declaration_name_range(content, decl) else {
                    continue;
                };
                let mut token_type = declaration_token_type(decl.kind);
                if decl.kind == DartDeclarationKind::Field
                    && decl
                        .parent_symbol_id
                        .as_deref()
                        .and_then(|parent| kind_by_symbol.get(parent))
                        .is_some_and(|kind| *kind == DartDeclarationKind::Enum)
                {
                    token_type = TK_ENUM_MEMBER;
                }
                let mut modifiers = SM_DECLARATION;
                let head = decl.declaration_span.as_ref().unwrap_or(&decl.span);
                let header = content
                    .get(head.byte_start.min(content.len())..head.byte_end.min(content.len()))
                    .unwrap_or("");
                if header.contains("static ") {
                    modifiers |= 1 << 1;
                }
                push_semantic_token(&mut tokens, content, start, end, token_type, modifiers);
            }
        }
        let mut parameter_names: BTreeSet<&str> = BTreeSet::new();
        for binding in snapshot
            .lexical_bindings()
            .iter()
            .filter(|binding| binding.source_path == path)
        {
            let token_type = match binding.kind {
                DartLexicalBindingKind::Parameter => {
                    parameter_names.insert(binding.name.as_str());
                    TK_PARAMETER
                }
                DartLexicalBindingKind::LocalVariable => TK_VARIABLE,
            };
            push_semantic_token(
                &mut tokens,
                content,
                binding.declaration_span.byte_start,
                binding.declaration_span.byte_end,
                token_type,
                SM_DECLARATION,
            );
        }
        for reference in snapshot
            .identifier_references()
            .iter()
            .filter(|reference| reference.source_path == path)
        {
            let Some((token_type, modifiers)) =
                reference_token_type(reference.kind, &reference.name, &parameter_names)
            else {
                continue;
            };
            push_semantic_token(
                &mut tokens,
                content,
                reference.span.byte_start,
                reference.span.byte_end,
                token_type,
                modifiers,
            );
        }
        let mut data = Vec::with_capacity(tokens.len() * 5);
        let (mut prev_line, mut prev_start) = (0u32, 0u32);
        for ((line, start), (token_type, modifiers, length)) in tokens {
            data.extend_from_slice(&[
                line - prev_line,
                if line == prev_line {
                    start - prev_start
                } else {
                    start
                },
                length,
                token_type,
                modifiers,
            ]);
            prev_line = line;
            prev_start = start;
        }
        Ok(Some(SemanticTokens {
            result_id: None,
            data,
        }))
    }

    /// LSP `textDocument/prepareRename`: resolves the symbol at the position
    /// and returns the exact word range plus the current name as the
    /// placeholder. Positions that resolve to nothing return `null`, which
    /// editors render as "cannot rename here".
    pub fn prepare_rename(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<PrepareRenameResult>, LspError> {
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
        if self
            .resolve_targets(&snapshot, &path, content, offset)
            .0
            .is_empty()
        {
            return Ok(None);
        }
        let Some((start, end)) = identifier_range_at(content, offset) else {
            return Ok(None);
        };
        let range = source_span_to_lsp_range(
            content,
            &dartscope_core::SourceSpan {
                byte_start: start,
                byte_end: end,
                start_line: 0,
                start_column: 1,
                end_line: 0,
                end_column: 1,
            },
        );
        Ok(Some(PrepareRenameResult {
            range,
            placeholder: Some(content[start..end].to_string()),
        }))
    }

    /// LSP `textDocument/rename`: replaces the resolved symbol's name at
    /// every occurrence the index can prove — the declaration span plus all
    /// references that resolve to the same target, across the open
    /// documents. Validation is deliberately conservative: the new name must
    /// be a valid non-reserved Dart identifier and must not collide with an
    /// existing declaration (workspace-wide for top-level symbols, per-file
    /// for members). Occurrences in files the editor has not opened are
    /// outside the index contract, so a rename is complete exactly when all
    /// referencing files are open.
    pub fn rename(
        &self,
        uri: &Url,
        position: Position,
        new_name: &str,
    ) -> Result<Option<WorkspaceEdit>, LspError> {
        if !is_valid_dart_name(new_name) {
            return Err(LspError::InvalidName(format!(
                "{new_name:?} is not a Dart identifier"
            )));
        }
        if DART_RESERVED_WORDS.contains(&new_name) {
            return Err(LspError::InvalidName(format!(
                "{new_name:?} is a reserved word"
            )));
        }
        let Some(content) = self.documents.get(uri) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let Some(offset) = lsp_position_to_byte_offset(content, position) else {
            return Err(LspError::InvalidPosition);
        };
        if let Some((start, end)) = identifier_range_at(content, offset)
            && &content[start..end] == new_name
        {
            return Ok(Some(WorkspaceEdit::default()));
        }
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let snapshot = index.snapshot();
        let path = document_path(uri);
        let (targets, _) = self.resolve_targets(&snapshot, &path, content, offset);
        if targets.is_empty() {
            return Ok(None);
        }
        let top_level = targets.iter().any(|target| match target {
            DartDefinitionTarget::Namespace(candidate) => snapshot
                .project()
                .files
                .iter()
                .flat_map(|file| file.declarations.iter())
                .any(|decl| {
                    decl.parent_symbol_id.is_none() && decl.symbol_id == candidate.symbol_id
                }),
            DartDefinitionTarget::Lexical(_) => false,
        });
        // Conservative collision guard: the new name must not be used by any
        // declaration in this file, and — for top-level symbols — by no
        // top-level declaration anywhere in the open workspace. Local
        // declarations never collide: shadowing is well-defined in Dart.
        for file in snapshot.project().files.iter() {
            for decl in &file.declarations {
                if decl.kind == DartDeclarationKind::LocalVariable || decl.name != new_name {
                    continue;
                }
                let collides = file.path == path || (top_level && decl.parent_symbol_id.is_none());
                if collides {
                    return Err(LspError::InvalidName(format!(
                        "{new_name:?} is already declared in {}",
                        file.path
                    )));
                }
            }
        }
        // Occurrences are the resolved reference facts plus each target's own
        // declaration name span. Replacements only ever cover the identifier:
        // a rename must never rewrite the surrounding declaration header,
        // so the recorded span is re-searched for the exact name.
        let ctx = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
        let refs = ctx.find_references(&targets);
        let mut changes: BTreeMap<Url, Vec<TextEdit>> = BTreeMap::new();
        for result in refs.results {
            for reference in result.references {
                let ref_uri = path_to_uri(&reference.source_path);
                let Some(ref_content) = self.documents.get(&ref_uri) else {
                    continue;
                };
                changes.entry(ref_uri).or_default().push(TextEdit {
                    range: source_span_to_lsp_range(ref_content, &reference.span),
                    new_text: new_name.to_string(),
                });
            }
        }
        for target in &targets {
            let (target_path, span, name) = match target {
                DartDefinitionTarget::Namespace(candidate) => (
                    candidate.declaration_path.as_str(),
                    &candidate.declaration_span,
                    candidate.name.as_str(),
                ),
                DartDefinitionTarget::Lexical(binding) => (
                    binding.source_path.as_str(),
                    &binding.declaration_span,
                    binding.name.as_str(),
                ),
            };
            let target_uri = path_to_uri(target_path);
            let Some(target_content) = self.documents.get(&target_uri) else {
                continue;
            };
            let Some(range) = declaration_span_name_range(target_content, name, span) else {
                continue;
            };
            changes.entry(target_uri).or_default().push(TextEdit {
                range,
                new_text: new_name.to_string(),
            });
        }
        for edits in changes.values_mut() {
            edits
                .sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
            edits.dedup();
        }
        if changes.is_empty() {
            return Ok(Some(WorkspaceEdit::default()));
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
        }))
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
        let (targets, occurrence) = self.resolve_targets(&snapshot, &path, content, offset);
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
        Ok(Some(Hover {
            contents: HoverContents::Array(contents),
            range: occurrence,
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

/// Semantic token legend published in the server capabilities. The legend
/// order defines the indexes used in the `textDocument/semanticTokens` data
/// encoding; extend only at the end to stay wire-compatible with caches.
const SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "class",
    "enum",
    "mixin",
    "extension",
    "type",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "method",
    "constructor",
];
const SEMANTIC_TOKEN_MODIFIERS: &[&str] = &["declaration", "static"];
const TK_CLASS: u32 = 0;
const TK_ENUM: u32 = 1;
const TK_MIXIN: u32 = 2;
const TK_EXTENSION: u32 = 3;
const TK_TYPE: u32 = 4;
const TK_PARAMETER: u32 = 5;
const TK_VARIABLE: u32 = 6;
const TK_PROPERTY: u32 = 7;
const TK_ENUM_MEMBER: u32 = 8;
const TK_FUNCTION: u32 = 9;
const TK_METHOD: u32 = 10;
const TK_CONSTRUCTOR: u32 = 11;
const SM_DECLARATION: u32 = 1 << 0;
const SM_STATIC: u32 = 1 << 1;

/// Reserved words that cannot be used as a Dart declaration name.
const DART_RESERVED_WORDS: &[&str] = &[
    "abstract",
    "as",
    "assert",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "covariant",
    "default",
    "deferred",
    "do",
    "dynamic",
    "else",
    "enum",
    "export",
    "extends",
    "extension",
    "external",
    "factory",
    "false",
    "final",
    "finally",
    "for",
    "get",
    "hide",
    "if",
    "implements",
    "import",
    "in",
    "interface",
    "is",
    "late",
    "library",
    "mixin",
    "new",
    "null",
    "on",
    "operator",
    "part",
    "required",
    "rethrow",
    "return",
    "set",
    "show",
    "static",
    "super",
    "switch",
    "sync",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

fn is_valid_dart_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_alphabetic())
        && chars.all(|c| c == '_' || c == '$' || c.is_alphanumeric())
}

fn identifier_cont_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$'
    ) || byte >= 0x80
}

/// Byte range of the Dart identifier containing (or adjacent to) `offset`.
/// Non-ASCII UTF-8 bytes count as continuations; this is an editor
/// affordance tokenizer, while the validity of any *new* name is checked
/// separately and strictly by [`is_valid_dart_name`].
fn identifier_range_at(content: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let at = offset.min(bytes.len());
    let mut start = at;
    while start > 0 && identifier_cont_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at;
    while end < bytes.len() && identifier_cont_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((start, end))
}

/// Identifier immediately before `end` (skipping whitespace), if any.
fn preceding_identifier(content: &str, end: usize) -> Option<(usize, usize)> {
    let head = content.get(..end)?.trim_end();
    let mut start = head.len();
    while start > 0 && identifier_cont_byte(content.as_bytes()[start - 1]) {
        start -= 1;
    }
    if start == head.len() {
        return None;
    }
    Some((start, head.len()))
}

/// LSP range of `name` inside a (possibly header-wide) recorded declaration
/// span. Rename edits use this so only the identifier is rewritten.
fn declaration_span_name_range(
    content: &str,
    name: &str,
    span: &dartscope_core::SourceSpan,
) -> Option<Range> {
    let text = content.get(span.byte_start.min(content.len())..span.byte_end.min(content.len()))?;
    let rel = text.find(name)?;
    let byte_start = span.byte_start + rel;
    Some(source_span_to_lsp_range(
        content,
        &dartscope_core::SourceSpan {
            byte_start,
            byte_end: byte_start + name.len(),
            start_line: 0,
            start_column: 1,
            end_line: 0,
            end_column: 1,
        },
    ))
}

fn declaration_token_type(kind: DartDeclarationKind) -> u32 {
    use DartDeclarationKind as D;
    match kind {
        D::Class => TK_CLASS,
        D::Enum => TK_ENUM,
        D::Mixin => TK_MIXIN,
        D::Extension | D::ExtensionType => TK_EXTENSION,
        D::Typedef => TK_TYPE,
        D::Function => TK_FUNCTION,
        D::Variable | D::LocalVariable => TK_VARIABLE,
        D::Method | D::Operator => TK_METHOD,
        D::Constructor => TK_CONSTRUCTOR,
        D::Field | D::Getter | D::Setter => TK_PROPERTY,
    }
}

fn reference_token_type(
    kind: DartIdentifierReferenceKind,
    name: &str,
    parameters: &BTreeSet<&str>,
) -> Option<(u32, u32)> {
    use DartIdentifierReferenceKind as R;
    Some(match kind {
        R::InvocationTarget => (TK_FUNCTION, 0),
        R::ConstructorTarget => (TK_CLASS, 0),
        R::MemberDeclarationInstance => (TK_METHOD, 0),
        R::MemberDeclarationStatic => (TK_METHOD, SM_STATIC),
        R::MemberInvocationInstance => (TK_METHOD, 0),
        R::MemberInvocationStatic => (TK_METHOD, SM_STATIC),
        R::MemberOperatorDeclaration | R::MemberOperatorInvocationInstance => (TK_METHOD, 0),
        R::MemberPropertyDeclarationInstance
        | R::MemberPropertyReadInstance
        | R::MemberPropertyWriteInstance => (TK_PROPERTY, 0),
        R::MemberPropertyDeclarationStatic
        | R::MemberPropertyReadStatic
        | R::MemberPropertyWriteStatic => (TK_PROPERTY, SM_STATIC),
        R::TypeAnnotation | R::ParameterType | R::ReturnType | R::VariableType => (TK_CLASS, 0),
        R::VariableRead | R::VariableWrite => {
            if parameters.contains(name) {
                (TK_PARAMETER, 0)
            } else {
                (TK_VARIABLE, 0)
            }
        }
    })
}

/// Record one semantic token, dropping ranges the LSP encoding cannot
/// express (multi-line) or zero-length spans produced by buffer drift.
/// First writer for a position wins, so declaration evidence takes priority
/// over reference facts on the same span.
fn push_semantic_token(
    tokens: &mut BTreeMap<(u32, u32), (u32, u32, u32)>,
    content: &str,
    byte_start: usize,
    byte_end: usize,
    token_type: u32,
    modifiers: u32,
) {
    let range = source_span_to_lsp_range(
        content,
        &dartscope_core::SourceSpan {
            byte_start,
            byte_end,
            start_line: 0,
            start_column: 1,
            end_line: 0,
            end_column: 1,
        },
    );
    if range.start.line != range.end.line {
        return;
    }
    let length = range.end.character.saturating_sub(range.start.character);
    if length == 0 {
        return;
    }
    tokens
        .entry((range.start.line, range.start.character))
        .or_insert((token_type, modifiers, length));
}

/// Receiver-type-aware resolution for qualified member accesses
/// (`receiver.member`) — the bounded LSP-side slice of DS-INDEX-006
/// receiver typing. It never invents types: the receiver must resolve to a
/// lexically visible parameter or local binding whose declared type is the
/// identifier token directly before the binding name, that type name must
/// match exactly one class-like declaration in the open workspace, and the
/// member must be directly declared on it. Anything weaker (field receivers,
/// inferred types, ambiguous names) stays unresolved rather than guessed.
fn receiver_type_targets_at(
    snapshot: &DartWorkspaceSnapshot,
    path: &str,
    content: &str,
    offset: usize,
) -> Vec<DartDefinitionTarget> {
    let Some((member_start, member_end)) = identifier_range_at(content, offset) else {
        return Vec::new();
    };
    let Some(before) = content.get(..member_start).map(str::trim_end) else {
        return Vec::new();
    };
    if content.as_bytes().get(before.len()) != Some(&b'.') {
        return Vec::new();
    }
    let Some((receiver_start, receiver_end)) = preceding_identifier(content, before.len()) else {
        return Vec::new();
    };
    let receiver = &content[receiver_start..receiver_end];
    if receiver == "this" || receiver == "super" {
        return Vec::new();
    }
    let member = &content[member_start..member_end];
    let Some(binding) = snapshot.lexical_bindings().iter().find(|binding| {
        binding.source_path == path
            && binding.name == receiver
            && binding.scope_span.byte_start <= offset
            && offset < binding.scope_span.byte_end
    }) else {
        return Vec::new();
    };
    let head = binding.declaration_span.byte_start;
    let Some((type_start, type_end)) = preceding_identifier(content, head) else {
        return Vec::new();
    };
    let type_name = &content[type_start..type_end];
    if DART_RESERVED_WORDS.contains(&type_name) {
        return Vec::new();
    }
    let mut owner: Option<(&str, &DartDeclaration)> = None;
    for file in snapshot.project().files.iter() {
        for decl in &file.declarations {
            if decl.parent_symbol_id.is_some()
                || decl.name != type_name
                || !matches!(
                    decl.kind,
                    DartDeclarationKind::Class
                        | DartDeclarationKind::Enum
                        | DartDeclarationKind::Mixin
                        | DartDeclarationKind::Extension
                        | DartDeclarationKind::ExtensionType
                )
            {
                continue;
            }
            if owner.is_some() {
                return Vec::new(); // ambiguous type name: refuse to guess
            }
            owner = Some((file.path.as_str(), decl));
        }
    }
    let Some((owner_path, owner)) = owner else {
        return Vec::new();
    };
    let Some(owner_symbol) = owner.symbol_id.as_deref() else {
        return Vec::new();
    };
    let member_decl = snapshot
        .project()
        .files
        .iter()
        .find(|file| file.path == owner_path)
        .and_then(|file| {
            file.declarations.iter().find(|decl| {
                decl.parent_symbol_id.as_deref() == Some(owner_symbol) && decl.name == member
            })
        });
    let Some(member_decl) = member_decl else {
        return Vec::new();
    };
    let declaration_span = member_decl
        .declaration_span
        .clone()
        .unwrap_or_else(|| member_decl.span.clone());
    vec![DartDefinitionTarget::Namespace(DartSymbolCandidate {
        name: member_decl.name.clone(),
        kind: member_decl.kind,
        symbol_id: member_decl.symbol_id.clone(),
        declaration_path: owner_path.to_string(),
        declaration_span,
        basis: if owner_path == path {
            DartSymbolResolutionBasis::SameFile
        } else {
            DartSymbolResolutionBasis::DirectImport
        },
    })]
}

/// Resolve a query offset that sits on a declaration identifier onto that
/// declaration itself. The parser deliberately does not emit reference facts
/// for declarations, so `find_definitions` cannot answer such queries; the
/// workspace snapshot's declaration inventory can, conservatively and exactly.
fn declaration_targets_at(
    snapshot: &DartWorkspaceSnapshot,
    path: &str,
    offset: usize,
    content: &str,
) -> Vec<DartDefinitionTarget> {
    let Some(file) = snapshot
        .project()
        .files
        .iter()
        .find(|file| file.path == path)
    else {
        return Vec::new();
    };
    file.declarations
        .iter()
        .filter(|decl| {
            declaration_name_range(content, decl)
                .is_some_and(|(start, end)| start <= offset && offset < end)
        })
        .map(|decl| {
            DartDefinitionTarget::Namespace(DartSymbolCandidate {
                name: decl.name.clone(),
                kind: decl.kind,
                symbol_id: decl.symbol_id.clone(),
                declaration_path: path.to_string(),
                declaration_span: decl
                    .declaration_span
                    .clone()
                    .unwrap_or_else(|| decl.span.clone()),
                basis: DartSymbolResolutionBasis::SameFile,
            })
        })
        .collect()
}

/// Short human label for one declaration kind, used as the `detail` column of
/// document symbols.
fn declaration_kind_label(kind: DartDeclarationKind) -> &'static str {
    use DartDeclarationKind as D;
    match kind {
        D::Class => "class",
        D::Mixin => "mixin",
        D::Enum => "enum",
        D::Extension => "extension",
        D::ExtensionType => "extension type",
        D::Typedef => "typedef",
        D::Function => "function",
        D::Variable => "variable",
        D::Method => "method",
        D::Constructor => "constructor",
        D::Field => "field",
        D::Getter => "getter",
        D::Setter => "setter",
        D::Operator => "operator",
        D::LocalVariable => "local variable",
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
        let tokens = result.capabilities.semantic_tokens_provider.unwrap();
        assert!(tokens.full);
        assert_eq!(tokens.range, Some(false));
        assert!(
            tokens
                .legend
                .token_types
                .contains(&"enumMember".to_string())
        );
        assert_eq!(
            tokens.legend.token_modifiers,
            vec!["declaration".to_string(), "static".to_string()]
        );
        match result.capabilities.rename_provider {
            Some(crate::types::OneOf::Right(options)) => {
                assert_eq!(options.prepare_provider, Some(true))
            }
            other => panic!("expected rename options, got {other:?}"),
        }
    }

    fn decode_semantic_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut out = Vec::new();
        let (mut line, mut start) = (0u32, 0u32);
        for chunk in data.chunks_exact(5) {
            line += chunk[0];
            start = if chunk[0] == 0 {
                start + chunk[1]
            } else {
                chunk[1]
            };
            out.push((line, start, chunk[2], chunk[3], chunk[4]));
        }
        out
    }

    #[test]
    fn semantic_tokens_cover_declarations_bindings_and_usages() {
        let content = "class Foo {\n  static int count = 0;\n  int twice(int x) { return x + x; }\n  void run() { print(count); }\n}\n";
        let (server, uri) = open_server("file:///lib/t.dart", content);
        let tokens = server.semantic_tokens(&uri).unwrap().unwrap();
        let decoded = decode_semantic_tokens(&tokens.data);
        // class declaration, static field declaration, parameter, and the
        // property usage inside `run` (without the declaration modifier).
        assert!(decoded.contains(&(0, 6, 3, TK_CLASS, SM_DECLARATION)));
        assert!(decoded.contains(&(1, 13, 5, TK_PROPERTY, SM_DECLARATION | SM_STATIC)));
        assert!(decoded.contains(&(2, 16, 1, TK_PARAMETER, SM_DECLARATION)));
        assert!(decoded.contains(&(3, 21, 5, TK_PROPERTY, 0)));
        // Delta encoding requires ascending document order.
        assert!(
            decoded
                .windows(2)
                .all(|w| (w[0].0, w[0].1) <= (w[1].0, w[1].1))
        );
    }

    #[test]
    fn semantic_tokens_on_unopened_document_is_an_error() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let uri = Url::parse("file:///lib/none.dart").unwrap();
        assert!(matches!(
            server.semantic_tokens(&uri),
            Err(LspError::DocumentNotOpen(_))
        ));
    }

    #[test]
    fn rename_replaces_name_spans_across_open_documents() {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        let a_uri = Url::parse("file:///lib/a.dart").unwrap();
        let b_uri = Url::parse("file:///lib/b.dart").unwrap();
        let a_content = "class Alpha {}\nvoid use() { Alpha(); }\n";
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
                text: "import 'a.dart';\nvoid go() { Alpha(); }\n".to_string(),
            },
        });
        let decl_pos = byte_offset_to_lsp_position(a_content, a_content.find("Alpha").unwrap());
        let edit = server.rename(&a_uri, decl_pos, "Omega").unwrap().unwrap();
        let changes = edit.changes.expect("edits produced");
        let a_edits = &changes[&a_uri];
        assert_eq!(a_edits.len(), 2, "declaration name + local usage");
        assert_eq!(
            a_edits[0].range,
            Range {
                start: Position {
                    line: 0,
                    character: 6
                },
                end: Position {
                    line: 0,
                    character: 11
                },
            }
        );
        assert!(a_edits.iter().all(|e| e.new_text == "Omega"));
        let b_edits = &changes[&b_uri];
        assert_eq!(b_edits.len(), 1);
        assert_eq!(b_edits[0].range.start.line, 1);
        // `prepareRename` reports the word under a usage position.
        let usage = a_content.find("Alpha(); }").unwrap();
        let usage_pos = byte_offset_to_lsp_position(a_content, usage);
        let prepared = server.prepare_rename(&a_uri, usage_pos).unwrap().unwrap();
        assert_eq!(prepared.placeholder.as_deref(), Some("Alpha"));
        assert_eq!(prepared.range.start.character, 13);
    }

    #[test]
    fn rename_validates_the_new_name() {
        let content = "class Alpha {}\nvoid use() { Alpha(); }\n";
        let (server, uri) = open_server("file:///lib/v.dart", content);
        let pos = byte_offset_to_lsp_position(content, content.find("Alpha").unwrap());
        assert!(matches!(
            server.rename(&uri, pos, "1bad"),
            Err(LspError::InvalidName(_))
        ));
        assert!(matches!(
            server.rename(&uri, pos, "class"),
            Err(LspError::InvalidName(_))
        ));
        assert!(matches!(
            server.rename(&uri, pos, "use"),
            Err(LspError::InvalidName(_))
        ));
        // Renaming to the current name is an honest no-op edit.
        let same = server.rename(&uri, pos, "Alpha").unwrap().unwrap();
        assert!(same.changes.is_none());
        // Positions that resolve to nothing produce no edit at all.
        let blank = Position {
            line: 0,
            character: 0,
        };
        assert!(server.rename(&uri, blank, "Fresh").unwrap().is_none());
    }

    #[test]
    fn receiver_typed_member_access_resolves_to_the_declared_member() {
        let content = "class Counter {\n  int value = 0;\n  int bump() { return 1; }\n}\nint measure(Counter c) { return c.bump() + c.value; }\n";
        let (server, uri) = open_server("file:///lib/r.dart", content);
        // The receiver `c` is a parameter whose declared type is `Counter`:
        // `c.bump` and `c.value` resolve onto the members of that class.
        let bump = content.rfind("bump").unwrap();
        let pos = byte_offset_to_lsp_position(content, bump);
        let locs = server.definition(&uri, pos).unwrap().unwrap();
        assert_eq!(locs[0].range.start.line, 2);
        let value = content.rfind("c.value").unwrap() + 2;
        let pos = byte_offset_to_lsp_position(content, value);
        let locs = server.definition(&uri, pos).unwrap().unwrap();
        assert_eq!(locs[0].range.start.line, 1);
        // Hover keeps the declaration-kind label for the resolved member.
        let hover = server.hover(&uri, pos).unwrap().unwrap();
        let HoverContents::Array(parts) = &hover.contents else {
            panic!("expected array");
        };
        let MarkedString::LanguageString(block) = &parts[0] else {
            panic!("expected language string");
        };
        assert!(block.value.contains("value"), "{}", block.value);
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
                version: Some(2),
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
                version: Some(2),
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
                version: Some(2),
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
        // `include_declaration: true` yields a superset that carries the
        // declaration span itself; the plain list still holds the usage.
        assert!(with_decl.iter().any(|l| l.range.start.line == 0));
        assert!(with_decl.iter().any(|l| l.range.start.line == 1));
        for loc in &without {
            assert!(with_decl.contains(loc));
        }
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
