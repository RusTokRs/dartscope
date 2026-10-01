//! LSP server state over `DartWorkspaceIndex`.

mod workspace;

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{self, AssertUnwindSafe};

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartFileInput,
    DartFileReferenceAnalysis, DartIdentifierReferenceKind, DartProjectInput, SourceSpan,
    normalize_path,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionTarget, DartWorkspaceIndex,
    DartWorkspaceResolutionContext, DartWorkspaceSnapshot,
};
use thiserror::Error;

pub use self::workspace::{NoWorkspace, WorkspaceFile, WorkspaceScan, WorkspaceSource};
use self::workspace::{ConfigFile, WorkspaceDocument};
use crate::coordinates::LineIndex;
use crate::types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, Hover, HoverContents,
    HoverProviderCapability, InitializeParams, InitializeResult, Location, MarkedString,
    NumberOrString, OneOf, Position, Range, ServerCapabilities, ServerInfo, SymbolKind,
    TextDocumentContentChangeEvent, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, Url,
};

/// The largest document, in bytes, that is analyzed for navigation.
///
/// Reference analysis is linear in the size of one file (about a quarter of a second per MiB in a
/// release build), but it runs again after every edit, on the thread that serves the editor. A larger
/// open document keeps its outline and diagnostics but is not part of navigation, and says so with a
/// diagnostic instead of making every keystroke wait for a multi-megabyte analysis. A larger file
/// that the client has not opened is not loaded into the workspace at all: files that size are
/// generated bindings and data tables, not code that anyone navigates.
pub const MAX_NAVIGATION_BYTES: usize = 1024 * 1024;

/// Code of the diagnostic that reports a document left out of navigation by its size.
pub const NAVIGATION_DISABLED_CODE: &str = "navigation_disabled_large_file";

/// Code of the diagnostic that reports a document whose analysis failed unexpectedly.
pub const ANALYSIS_FAILED_CODE: &str = "analysis_failed";

/// How deep a document outline nests; declarations only nest two levels (type, member), so this
/// only bounds a malformed parent chain.
const MAX_OUTLINE_DEPTH: usize = 8;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("document not open: {0}")]
    DocumentNotOpen(String),
    #[error("invalid position")]
    InvalidPosition,
    #[error("not initialized")]
    NotInitialized,
}

/// One document the client has open: its URI exactly as the client wrote it, so that results can
/// name it the way the client does, and its current text.
#[derive(Debug)]
struct OpenDocument {
    uri: Url,
    version: i32,
    text: String,
    /// The analysis of the current text panicked; the document is not in the index.
    analysis_failed: bool,
}

/// In-memory LSP server with incremental document sync and index-backed navigation.
///
/// The server does not touch the filesystem: `root` is the LSP workspace root
/// (e.g. `file:///workspace`), every `textDocument/*` notification carries the content, and the
/// files of the project that the client has not opened are handed to it as text through
/// [`DartLspServer::load_workspace`] and [`DartLspServer::update_workspace_file`] (the stdio binary
/// reads them with `crate::fs_workspace::FsWorkspace`). The index holds the open documents and those
/// files, an open document taking the place of its file, so navigation sees the whole project and the
/// unsaved text of the buffers. A change re-analyzes the one document it touches and updates the
/// incremental index in place; the resolution context that answers queries is built lazily, once
/// per index generation. All positions are converted via `crate::coordinates`, so `\n`, `\r\n`,
/// `\r` and UTF-16 surrogate pairs round-trip.
pub struct DartLspServer {
    root: String,
    root_url: Option<Url>,
    /// The directories the client works in, as filesystem paths with `/` separators.
    roots: Vec<String>,
    /// The client can watch files for the server (`workspace/didChangeWatchedFiles`).
    watches_files: bool,
    /// Open documents by normalized path (see [`uri_to_path`]), which is also their index path.
    documents: HashMap<String, OpenDocument>,
    /// The Dart files of the project that were loaded from disk, by index path.
    workspace: HashMap<String, WorkspaceDocument>,
    /// The `pubspec.yaml` and `package_config.json` files of the project, by index path.
    configs: BTreeMap<String, ConfigFile>,
    index: DartWorkspaceIndex,
    /// Resolution context of the current index generation; reset by every index update.
    context: OnceCell<DartWorkspaceResolutionContext>,
    initialized: bool,
    shutdown_requested: bool,
}

impl DartLspServer {
    pub fn new(root: impl Into<String>) -> Self {
        let root = root.into();
        Self {
            index: empty_index(&root),
            root,
            root_url: None,
            roots: Vec::new(),
            watches_files: false,
            documents: HashMap::new(),
            workspace: HashMap::new(),
            configs: BTreeMap::new(),
            context: OnceCell::new(),
            initialized: false,
            shutdown_requested: false,
        }
    }

    pub fn initialize(&mut self, params: InitializeParams) -> Result<InitializeResult, LspError> {
        let folders = workspace_folders(&params);
        if let Some(root_uri) = params.root_uri {
            self.root = uri_to_path(&root_uri);
            self.roots = vec![file_system_path(&root_uri)];
            self.root_url = Some(root_uri);
        } else if let Some(root_path) = params.root_path {
            self.root = index_path(&root_path);
            self.roots = vec![normalize_path(root_path)];
        }
        if !folders.is_empty() {
            if self.root_url.is_none() {
                self.root = uri_to_path(&folders[0]);
            }
            self.roots = folders.iter().map(file_system_path).collect();
        }
        self.watches_files = params
            .capabilities
            .pointer("/workspace/didChangeWatchedFiles/dynamicRegistration")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let _ = self.index.update_root(self.root.clone());
        self.context = OnceCell::new();
        self.initialized = true;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::Incremental),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
            },
            server_info: Some(ServerInfo {
                name: "dartscope-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    pub fn initialized(&mut self) {
        // No-op: diagnostics are published when a document is opened or changed.
    }

    /// The workspace root URI the client reported in `initialize`, if any.
    pub fn root_url(&self) -> Option<&Url> {
        self.root_url.as_ref()
    }

    /// Whether `initialize` has been handled.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Whether `shutdown` has been requested; after it only `exit` is meaningful.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_requested
    }

    pub fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }
        self.shutdown_requested = true;
        Ok(())
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let item = params.text_document;
        let path = uri_to_path(&item.uri);
        self.documents.insert(
            path.clone(),
            OpenDocument {
                uri: item.uri,
                version: item.version,
                text: item.text,
                analysis_failed: false,
            },
        );
        self.reindex(&path);
    }

    /// Applies the changes in order. A change whose range lies outside the document is clamped to
    /// it, as the protocol asks, and never replaces the whole text.
    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        let path = uri_to_path(&params.text_document.uri);
        let Some(document) = self.documents.get_mut(&path) else {
            return;
        };
        for change in params.content_changes {
            apply_change(&mut document.text, change);
        }
        document.version = params.text_document.version;
        self.reindex(&path);
    }

    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        let path = uri_to_path(&params.text_document.uri);
        if self.documents.remove(&path).is_some() {
            // The file on disk, if the project has one, takes the place of the buffer.
            self.reindex_from_disk(&path);
        }
    }

    /// The version the client last reported for an open document.
    pub fn document_version(&self, uri: &Url) -> Option<i32> {
        self.documents
            .get(&uri_to_path(uri))
            .map(|document| document.version)
    }

    pub fn definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<Location>>, LspError> {
        let (_, Some(resolution)) = self.resolve_at(uri, position)? else {
            return Ok(None);
        };
        let mut locator = Locator::new(&self.documents, &self.workspace);
        let locations: Vec<Location> = resolution
            .targets
            .iter()
            .filter_map(|target| {
                let (path, span, name) = target_declaration(target);
                locator.name_location(path, span, name)
            })
            .collect();
        Ok((!locations.is_empty()).then_some(locations))
    }

    /// References to the symbol at `position`, without its declaration.
    pub fn references(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<Location>>, LspError> {
        self.references_with_declaration(uri, position, false)
    }

    /// References to the symbol at `position`; `include_declaration` adds where it is declared.
    pub fn references_with_declaration(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<Location>>, LspError> {
        let (_, Some(resolution)) = self.resolve_at(uri, position)? else {
            return Ok(None);
        };
        let mut locator = Locator::new(&self.documents, &self.workspace);
        let mut locations = Vec::new();
        let found = self
            .resolution_context()
            .find_references(&resolution.targets);
        for result in &found.results {
            for reference in &result.references {
                // The index reports the declaration of a member as a reference to it; whether it
                // belongs in the answer is the client's choice.
                if !include_declaration && is_declaration_reference(reference.kind) {
                    continue;
                }
                locations.extend(locator.span_location(&reference.source_path, &reference.span));
            }
        }
        if include_declaration {
            for target in &resolution.targets {
                let (path, span, name) = target_declaration(target);
                let Some(declaration) = locator.name_location(path, span, name) else {
                    continue;
                };
                // A member declaration is already there as a reference.
                if !locations.iter().any(|known| {
                    known.uri == declaration.uri && overlaps(&known.range, &declaration.range)
                }) {
                    locations.push(declaration);
                }
            }
        }
        locations.sort_by(compare_locations);
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        Ok(Some(locations))
    }

    pub fn hover(&self, uri: &Url, position: Position) -> Result<Option<Hover>, LspError> {
        let (document, Some(resolution)) = self.resolve_at(uri, position)? else {
            return Ok(None);
        };
        let contents = resolution
            .targets
            .iter()
            .map(|target| {
                let (name, kind) = match target {
                    DartDefinitionTarget::Namespace(candidate) => {
                        (candidate.name.as_str(), format!("{:?}", candidate.kind))
                    }
                    DartDefinitionTarget::Lexical(binding) => {
                        (binding.name.as_str(), format!("{:?}", binding.kind))
                    }
                };
                MarkedString::String(format!("{kind} {name}"))
            })
            .collect();
        let lines = LineIndex::new(&document.text);
        let range = resolution.references.first().map(|reference| Range {
            start: lines.position(reference.span.byte_start),
            end: lines.position(reference.span.byte_end),
        });
        Ok(Some(Hover {
            contents: HoverContents::Array(contents),
            range,
        }))
    }

    /// The outline of an open document: types with their members, then top-level functions,
    /// variables and accessors. Locals are not part of an outline.
    pub fn document_symbols(
        &self,
        uri: &Url,
        _params: DocumentSymbolParams,
    ) -> Result<Option<Vec<DocumentSymbol>>, LspError> {
        let path = uri_to_path(uri);
        let Some(document) = self.documents.get(&path) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let snapshot = self.index.snapshot();
        let Some(file) = find_file(&snapshot, &path) else {
            return Ok(None);
        };
        Ok(Some(outline(file, &document.text)))
    }

    /// The diagnostics of an open document, as LSP diagnostics.
    pub fn diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let path = uri_to_path(uri);
        let Some(document) = self.documents.get(&path) else {
            return Vec::new();
        };
        let snapshot = self.index.snapshot();
        let Some(file) = find_file(&snapshot, &path) else {
            return if document.analysis_failed {
                vec![analysis_failed_diagnostic()]
            } else {
                Vec::new()
            };
        };
        let lines = LineIndex::new(&document.text);
        let mut diagnostics: Vec<Diagnostic> = file
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let range = diagnostic.span.as_ref().map_or(
                    Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    |span| Range {
                        start: lines.position(span.byte_start),
                        end: lines.position(span.byte_end),
                    },
                );
                let severity = match diagnostic.severity {
                    dartscope_core::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    dartscope_core::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                    dartscope_core::DiagnosticSeverity::Info => DiagnosticSeverity::INFORMATION,
                };
                Diagnostic {
                    range,
                    severity: Some(severity),
                    code: Some(NumberOrString::String(diagnostic.code.clone())),
                    source: Some("dartscope".to_string()),
                    message: diagnostic.message.clone(),
                }
            })
            .collect();
        if document.text.len() > MAX_NAVIGATION_BYTES {
            diagnostics.push(Diagnostic {
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
                severity: Some(DiagnosticSeverity::INFORMATION),
                code: Some(NumberOrString::String(NAVIGATION_DISABLED_CODE.to_string())),
                source: Some("dartscope".to_string()),
                message: format!(
                    "navigation is off for this file because it is larger than {} KiB; the outline and diagnostics still work",
                    MAX_NAVIGATION_BYTES / 1024
                ),
            });
        }
        diagnostics
    }

    /// Re-analyzes one open document and updates the index in place.
    ///
    /// A bug in the analysis of one text must not end the session of an editor: the document is left
    /// out of the index, and says so in its diagnostics, until its text changes.
    fn reindex(&mut self, path: &str) {
        let Some(document) = self.documents.get(path) else {
            return;
        };
        let text = document.text.clone();
        let analysis = panic::catch_unwind(AssertUnwindSafe(|| analyze_document(path, text)));
        let failed = analysis.is_err();
        match analysis {
            Ok(analysis) => {
                let _ = self.index.upsert_file_with_references(analysis);
            }
            Err(_) => {
                let _ = self.index.remove_file(path);
            }
        }
        if let Some(document) = self.documents.get_mut(path) {
            document.analysis_failed = failed;
        }
        self.context = OnceCell::new();
    }

    /// The resolution context of the current index generation.
    fn resolution_context(&self) -> &DartWorkspaceResolutionContext {
        self.context
            .get_or_init(|| DartWorkspaceResolutionContext::from_snapshot(&self.index.snapshot()))
    }

    /// Resolves the definition at a position of an open document; the position may lie beyond the
    /// text, in which case it is clamped. `None` when nothing resolves there.
    fn resolve_at(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<(&OpenDocument, Option<DartDefinitionResolution>), LspError> {
        #[cfg(test)]
        fault::fail_query_if_requested();
        let path = uri_to_path(uri);
        let Some(document) = self.documents.get(&path) else {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        };
        let offset = LineIndex::new(&document.text).offset_clamped(position);
        let batch = self
            .resolution_context()
            .find_definitions(&[DartDefinitionQuery::new(path, offset)]);
        let resolution = batch
            .resolutions
            .into_iter()
            .next()
            .filter(|resolution| !resolution.targets.is_empty());
        Ok((document, resolution))
    }
}

/// The analysis of one document: with references, unless the text is too large for them.
fn analyze_document(path: &str, text: String) -> DartFileReferenceAnalysis {
    #[cfg(test)]
    fault::fail_analysis_if_requested();
    let too_large = text.len() > MAX_NAVIGATION_BYTES;
    let input = DartFileInput::new(path, text);
    if too_large {
        DartFileReferenceAnalysis {
            file: dartscope_parse::analyze_file(input),
            references: Vec::new(),
            bindings: Vec::new(),
        }
    } else {
        dartscope_parse::analyze_file_with_references(input)
    }
}

/// Test-only switches that make the analysis or a query panic, to prove that the server survives.
#[cfg(test)]
pub(crate) mod fault {
    use std::cell::Cell;

    thread_local! {
        static ANALYSIS: Cell<bool> = const { Cell::new(false) };
        static QUERY: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn set_analysis(on: bool) {
        ANALYSIS.with(|flag| flag.set(on));
    }

    pub(crate) fn set_query(on: bool) {
        QUERY.with(|flag| flag.set(on));
    }

    pub(super) fn fail_analysis_if_requested() {
        assert!(!ANALYSIS.with(Cell::get), "injected analysis failure");
    }

    pub(super) fn fail_query_if_requested() {
        assert!(!QUERY.with(Cell::get), "injected query failure");
    }
}

/// An index without any file, rooted at `root`.
fn empty_index(root: &str) -> DartWorkspaceIndex {
    DartWorkspaceIndex::from_reference_project(dartscope_parse::analyze_project_with_references(
        DartProjectInput::new(root, Vec::new(), Vec::new()),
    ))
}

/// The index path of a document URI: the decoded path of the URI with `/` separators and without
/// the leading `/`, so `file:///C%3A/proj/a.dart` becomes `C:/proj/a.dart`. The mapping only has to
/// be stable; results are reported with the URI the client sent, not with this path.
fn uri_to_path(uri: &Url) -> String {
    index_path(&file_system_path(uri))
}

/// The decoded path of a URI with `/` separators, as it is written for the filesystem.
fn file_system_path(uri: &Url) -> String {
    let path = match uri.to_file_path() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => uri.path().to_string(),
    };
    normalize_path(path)
}

/// The index path of a filesystem path: normalized, without the leading `/`, and with the drive
/// letter of a Windows path in lower case, because editors spell it both ways (`C:` and `c:`) for
/// one file.
fn index_path(path: &str) -> String {
    let normalized = normalize_path(path.to_string());
    let trimmed = normalized.trim_start_matches('/');
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let mut lowered = trimmed.to_string();
        lowered[..1].make_ascii_lowercase();
        return lowered;
    }
    trimmed.to_string()
}

/// The folders of `workspaceFolders` in the parameters of `initialize`.
fn workspace_folders(params: &InitializeParams) -> Vec<Url> {
    params
        .extra
        .get("workspaceFolders")
        .and_then(serde_json::Value::as_array)
        .map(|folders| {
            folders
                .iter()
                .filter_map(|folder| folder.get("uri")?.as_str())
                .filter_map(|uri| Url::parse(uri).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Applies one `didChange` content change to the text of a document.
///
/// A change without a range replaces the text. A range is clamped to the text, so a position past
/// the end of a line is the end of that line and a line past the end is the end of the text, and a
/// reversed range is read in order.
fn apply_change(text: &mut String, change: TextDocumentContentChangeEvent) {
    let Some(range) = change.range else {
        *text = change.text;
        return;
    };
    let lines = LineIndex::new(text);
    let mut start = lines.offset_clamped(range.start);
    let mut end = lines.offset_clamped(range.end);
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    text.replace_range(start..end, &change.text);
}

/// Where a definition target is declared: its path, the declaration span and its name.
fn target_declaration(target: &DartDefinitionTarget) -> (&str, &SourceSpan, &str) {
    match target {
        DartDefinitionTarget::Namespace(candidate) => (
            &candidate.declaration_path,
            &candidate.declaration_span,
            &candidate.name,
        ),
        DartDefinitionTarget::Lexical(binding) => (
            &binding.source_path,
            &binding.declaration_span,
            &binding.name,
        ),
    }
}

/// Reference kinds that the index uses for the declaration of a member rather than for a use.
fn is_declaration_reference(kind: DartIdentifierReferenceKind) -> bool {
    matches!(
        kind,
        DartIdentifierReferenceKind::MemberDeclarationInstance
            | DartIdentifierReferenceKind::MemberDeclarationStatic
            | DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
            | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
            | DartIdentifierReferenceKind::MemberOperatorDeclaration
    )
}

/// Whether two ranges share a position; ranges that only touch do not overlap.
fn overlaps(left: &Range, right: &Range) -> bool {
    let key = |position: &Position| (position.line, position.character);
    key(&left.start) < key(&right.end) && key(&right.start) < key(&left.end)
}

fn analysis_failed_diagnostic() -> Diagnostic {
    let start = Position {
        line: 0,
        character: 0,
    };
    Diagnostic {
        range: Range {
            start,
            end: start,
        },
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String(ANALYSIS_FAILED_CODE.to_string())),
        source: Some("dartscope".to_string()),
        message: "the analysis of this document failed unexpectedly, so its outline and navigation are off until it changes; please report the text that causes this".to_string(),
    }
}

fn compare_locations(left: &Location, right: &Location) -> Ordering {
    let key = |location: &Location| {
        (
            location.uri.as_str().to_string(),
            location.range.start.line,
            location.range.start.character,
            location.range.end.line,
            location.range.end.character,
        )
    };
    key(left).cmp(&key(right))
}

/// The analysis of one path in a workspace snapshot; its files are ordered by path.
fn find_file<'a>(snapshot: &'a DartWorkspaceSnapshot, path: &str) -> Option<&'a DartFileAnalysis> {
    let files = &snapshot.project().files;
    files
        .binary_search_by(|file| file.path.as_str().cmp(path))
        .ok()
        .map(|index| &files[index])
}

/// Converts byte spans of open documents and of files loaded from disk to locations, building the
/// line index of each at most once per request.
struct Locator<'a> {
    documents: &'a HashMap<String, OpenDocument>,
    workspace: &'a HashMap<String, WorkspaceDocument>,
    lines: HashMap<&'a str, LineIndex<'a>>,
}

impl<'a> Locator<'a> {
    fn new(
        documents: &'a HashMap<String, OpenDocument>,
        workspace: &'a HashMap<String, WorkspaceDocument>,
    ) -> Self {
        Self {
            documents,
            workspace,
            lines: HashMap::new(),
        }
    }

    /// The key, URI and text of a file the server has the text of: the open buffer if there is one,
    /// else the file as loaded from disk.
    fn source(&self, path: &str) -> Option<(&'a str, &'a Url, &'a str)> {
        if let Some((key, document)) = self.documents.get_key_value(path) {
            return Some((key.as_str(), &document.uri, document.text.as_str()));
        }
        let (key, document) = self.workspace.get_key_value(path)?;
        Some((key.as_str(), &document.uri, document.text.as_str()))
    }

    /// The bytes `start..end` of a document, with the URI the client used for it (or the one built
    /// from its path, for a file the client has not opened). `None` when the server has no text of
    /// the file, which leaves nothing to convert the offsets against.
    fn location(&mut self, path: &str, start: usize, end: usize) -> Option<Location> {
        let (key, uri, text) = self.source(path)?;
        let lines = self
            .lines
            .entry(key)
            .or_insert_with(|| LineIndex::new(text));
        Some(Location {
            uri: uri.clone(),
            range: Range {
                start: lines.position(start),
                end: lines.position(end),
            },
        })
    }

    fn span_location(&mut self, path: &str, span: &SourceSpan) -> Option<Location> {
        self.location(path, span.byte_start, span.byte_end)
    }

    /// The declared name within a declaration span, or the whole span when the name is not found.
    fn name_location(&mut self, path: &str, span: &SourceSpan, name: &str) -> Option<Location> {
        let (_, _, text) = self.source(path)?;
        let (start, end) =
            find_name(text, span, name).unwrap_or((span.byte_start, span.byte_end));
        self.location(path, start, end)
    }
}

/// The first whole-identifier occurrence of `name` inside `span`, as a byte range of `text`.
fn find_name(text: &str, span: &SourceSpan, name: &str) -> Option<(usize, usize)> {
    if name.is_empty() {
        return None;
    }
    let region = text.get(span.byte_start..span.byte_end)?;
    let mut from = 0;
    while let Some(found) = region[from..].find(name) {
        let start = from + found;
        let end = start + name.len();
        let before = region[..start].chars().next_back();
        let after = region[end..].chars().next();
        if !before.is_some_and(is_identifier_character)
            && !after.is_some_and(is_identifier_character)
        {
            return Some((span.byte_start + start, span.byte_start + end));
        }
        from = start + region[start..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '$'
}

/// The outline of one file: declarations grouped under their parent, locals left out.
fn outline(file: &DartFileAnalysis, text: &str) -> Vec<DocumentSymbol> {
    let declarations = || {
        file.declarations
            .iter()
            .filter(|declaration| declaration.kind != DartDeclarationKind::LocalVariable)
    };
    let known: HashSet<&str> = declarations()
        .filter_map(|declaration| declaration.symbol_id.as_deref())
        .collect();
    let mut children: HashMap<&str, Vec<&DartDeclaration>> = HashMap::new();
    let mut roots = Vec::new();
    for declaration in declarations() {
        match declaration.parent_symbol_id.as_deref() {
            Some(parent) if known.contains(parent) => {
                children.entry(parent).or_default().push(declaration);
            }
            _ => roots.push(declaration),
        }
    }
    let lines = LineIndex::new(text);
    let builder = OutlineBuilder {
        text,
        lines: &lines,
        children: &children,
    };
    roots
        .into_iter()
        .map(|declaration| builder.symbol(declaration, None, 0))
        .collect()
}

struct OutlineBuilder<'a> {
    text: &'a str,
    lines: &'a LineIndex<'a>,
    children: &'a HashMap<&'a str, Vec<&'a DartDeclaration>>,
}

impl OutlineBuilder<'_> {
    fn range(&self, start: usize, end: usize) -> Range {
        Range {
            start: self.lines.position(start),
            end: self.lines.position(end),
        }
    }

    fn symbol(
        &self,
        declaration: &DartDeclaration,
        parent: Option<&DartDeclaration>,
        depth: usize,
    ) -> DocumentSymbol {
        let span = declaration
            .declaration_span
            .as_ref()
            .unwrap_or(&declaration.span);
        let range = self.range(span.byte_start, span.byte_end);
        // The selection range must lie inside the range: the name where it can be found in the
        // declaration, otherwise the whole declaration.
        let selection_range = find_name(self.text, span, &declaration.name)
            .map_or_else(|| range.clone(), |(start, end)| self.range(start, end));
        let children = declaration
            .symbol_id
            .as_deref()
            .and_then(|id| self.children.get(id))
            .filter(|_| depth < MAX_OUTLINE_DEPTH)
            .map(|members| {
                members
                    .iter()
                    .map(|member| self.symbol(member, Some(declaration), depth + 1))
                    .collect::<Vec<_>>()
            });
        DocumentSymbol {
            name: declaration.name.clone(),
            detail: None,
            kind: symbol_kind(declaration.kind, parent.map(|parent| parent.kind)),
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children,
        }
    }
}

fn symbol_kind(kind: DartDeclarationKind, parent: Option<DartDeclarationKind>) -> SymbolKind {
    match kind {
        DartDeclarationKind::Class
        | DartDeclarationKind::Mixin
        | DartDeclarationKind::ExtensionType => SymbolKind::Class,
        DartDeclarationKind::Enum => SymbolKind::Enum,
        DartDeclarationKind::Extension | DartDeclarationKind::Typedef => SymbolKind::Interface,
        DartDeclarationKind::Function => SymbolKind::Function,
        DartDeclarationKind::Method => SymbolKind::Method,
        DartDeclarationKind::Constructor => SymbolKind::Constructor,
        DartDeclarationKind::Field if parent == Some(DartDeclarationKind::Enum) => {
            SymbolKind::EnumMember
        }
        DartDeclarationKind::Field => SymbolKind::Field,
        DartDeclarationKind::Getter | DartDeclarationKind::Setter => SymbolKind::Property,
        DartDeclarationKind::Operator => SymbolKind::Operator,
        DartDeclarationKind::Variable | DartDeclarationKind::LocalVariable => SymbolKind::Variable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::byte_offset_to_lsp_position;
    use crate::types::{TextDocumentIdentifier, TextDocumentItem, VersionedTextDocumentIdentifier};

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn started() -> DartLspServer {
        let mut server = DartLspServer::new(".");
        server.initialize(InitializeParams::default()).unwrap();
        server
    }

    fn open(server: &mut DartLspServer, uri: &Url, text: &str) {
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "dart".to_string(),
                version: 1,
                text: text.to_string(),
            },
        });
    }

    fn change(
        uri: &Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> DidChangeTextDocumentParams {
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes: changes,
        }
    }

    fn edit(start: (u32, u32), end: (u32, u32), text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: start.0,
                    character: start.1,
                },
                end: Position {
                    line: end.0,
                    character: end.1,
                },
            }),
            range_length: None,
            text: text.to_string(),
        }
    }

    fn text_of<'a>(server: &'a DartLspServer, uri: &Url) -> &'a str {
        &server.documents.get(&uri_to_path(uri)).unwrap().text
    }

    fn symbols_params(uri: &Url) -> DocumentSymbolParams {
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    #[test]
    fn initialize_returns_capabilities() {
        let mut server = DartLspServer::new("/tmp");
        let params = InitializeParams::default();
        let result = server.initialize(params).unwrap();
        assert!(result.capabilities.definition_provider.is_some());
    }

    #[test]
    fn did_open_and_definition_round_trip() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        open(&mut server, &uri, content);
        // Position of the `Foo` constructor call in `Foo().bar()`: it resolves to the class.
        // A member access on an expression receiver (`Foo().bar`) needs receiver inference, which
        // DartScope does not perform (docs/development/audit-findings-2026-09-30.md, section 14.1).
        let offset = content.find("Foo().bar").unwrap();
        let pos = byte_offset_to_lsp_position(content, offset);
        let locs = server.definition(&uri, pos).unwrap();
        assert!(locs.is_some());
        let locs = locs.unwrap();
        assert!(!locs.is_empty());
    }

    #[test]
    fn diagnostics_published_for_unsupported_syntax() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        // The Dart 3.13 concise constructor form is a leading `new`; `Foo.new();` is an ordinary
        // unnamed constructor and is supported.
        open(&mut server, &uri, "class Foo { new(); }");
        let diags = server.diagnostics(&uri);
        assert!(!diags.is_empty());
    }

    #[test]
    fn incremental_change_applies_utf16() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        open(&mut server, &uri, "class A {}\n");
        // Insert "😀" at line 0 char 6 (after "class ")
        server.did_change(change(&uri, 2, vec![edit((0, 6), (0, 6), "😀")]));
        assert!(text_of(&server, &uri).contains("😀"));
        assert_eq!(server.document_version(&uri), Some(2));
    }

    #[test]
    fn a_change_outside_the_document_is_clamped_and_never_replaces_the_text() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        open(&mut server, &uri, "class A {}\nclass B {}\n");

        // Past the end of line 0: appended at the end of that line.
        server.did_change(change(&uri, 2, vec![edit((0, 99), (0, 99), " // a")]));
        assert_eq!(text_of(&server, &uri), "class A {} // a\nclass B {}\n");

        // A line beyond the text: appended at the end of the text.
        server.did_change(change(&uri, 3, vec![edit((50, 0), (50, 0), "class C {}")]));
        assert_eq!(
            text_of(&server, &uri),
            "class A {} // a\nclass B {}\nclass C {}"
        );

        // A reversed range is read in order: this replaces the characters 6..10, `A {}`.
        server.did_change(change(&uri, 4, vec![edit((0, 10), (0, 6), "X")]));
        assert_eq!(
            text_of(&server, &uri),
            "class X // a\nclass B {}\nclass C {}"
        );
    }

    #[test]
    fn changes_apply_in_order_against_the_text_they_follow() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        open(&mut server, &uri, "class A {}\n");
        // The second range refers to the text after the first edit: `Renamed` fills 6..13.
        server.did_change(change(
            &uri,
            2,
            vec![
                edit((0, 6), (0, 7), "Renamed"),
                edit((0, 6), (0, 13), "Box"),
            ],
        ));
        assert_eq!(text_of(&server, &uri), "class Box {}\n");

        // An edit after a full replacement refers to the replaced text.
        server.did_change(change(
            &uri,
            3,
            vec![
                TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class Whole {}".to_string(),
                },
                edit((0, 5), (0, 5), "!"),
            ],
        ));
        assert_eq!(text_of(&server, &uri), "class! Whole {}");
    }

    #[test]
    fn rapid_file_replacement_keeps_index_consistent() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        for i in 0..10 {
            let content = format!("class A{i} {{}}\n");
            if i == 0 {
                open(&mut server, &uri, &content);
            } else {
                server.did_change(change(
                    &uri,
                    i,
                    vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: content.clone(),
                    }],
                ));
            }
            // Every intermediate state is queryable, and the index holds exactly the latest text.
            let symbols = server
                .document_symbols(&uri, symbols_params(&uri))
                .unwrap()
                .unwrap();
            assert_eq!(symbols.len(), 1);
            assert_eq!(symbols[0].name, format!("A{i}"));
        }
        assert!(text_of(&server, &uri).contains("class A9"));
    }

    #[test]
    fn hover_returns_kind_and_name() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        let content = "class Foo { void bar() {} }\nvoid main() { Foo().bar(); }";
        open(&mut server, &uri, content);
        let offset = content.find("bar()").unwrap();
        let pos = byte_offset_to_lsp_position(content, offset);
        let hover = server.hover(&uri, pos).unwrap();
        assert!(hover.is_some());
        let hover = hover.unwrap();
        match hover.contents {
            HoverContents::Array(arr) => assert!(!arr.is_empty()),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn results_name_documents_with_the_uri_the_client_sent() {
        let mut server = started();
        // A path with a space and a non-ASCII letter, percent-encoded the way editors write it.
        let lib = url("file:///my%20proj/lib/%C3%BCber.dart");
        let main = url("file:///my%20proj/lib/main.dart");
        open(&mut server, &lib, "class Widget {}\n");
        let main_text = "import '\u{fc}ber.dart';\nvoid run() { Widget(); }\n";
        open(&mut server, &main, main_text);

        let offset = main_text.find("Widget").unwrap();
        let position = byte_offset_to_lsp_position(main_text, offset);
        let locations = server.definition(&main, position).unwrap().unwrap();

        assert!(
            locations.iter().all(|location| location.uri == lib),
            "{locations:?}"
        );
        assert_eq!(locations[0].uri, lib);
        // The range is the name inside the declaration, converted against the declaring file.
        assert_eq!(
            locations[0].range,
            Range {
                start: Position {
                    line: 0,
                    character: 6
                },
                end: Position {
                    line: 0,
                    character: 12
                },
            }
        );
    }

    #[test]
    fn references_include_the_declaration_only_when_asked() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        let text = "class Widget {}\nvoid a() { Widget(); }\nvoid b() { Widget(); }\n";
        open(&mut server, &uri, text);
        let at = byte_offset_to_lsp_position(text, text.find("Widget();").unwrap());

        let without = server
            .references_with_declaration(&uri, at, false)
            .unwrap()
            .unwrap();
        let with = server
            .references_with_declaration(&uri, at, true)
            .unwrap()
            .unwrap();

        let declaration = Range {
            start: Position {
                line: 0,
                character: 6,
            },
            end: Position {
                line: 0,
                character: 12,
            },
        };
        assert!(without.iter().all(|location| location.range != declaration));
        assert!(with.iter().any(|location| location.range == declaration));
        assert_eq!(with.len(), without.len() + 1);
        assert!(without.len() >= 2, "{without:?}");
    }

    #[test]
    fn a_member_declaration_is_not_a_reference_unless_the_declaration_is_included() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        let text = "class Box {\n  void run() {}\n  void twice() {\n    this.run();\n    this.run();\n  }\n}\n";
        open(&mut server, &uri, text);
        let at = byte_offset_to_lsp_position(text, text.find("run();").unwrap());
        let declaration_line = 1;

        let without = server
            .references_with_declaration(&uri, at, false)
            .unwrap()
            .unwrap();
        let with = server
            .references_with_declaration(&uri, at, true)
            .unwrap()
            .unwrap();

        assert!(
            without
                .iter()
                .all(|location| location.range.start.line != declaration_line),
            "{without:?}"
        );
        assert_eq!(without.len(), 2, "{without:?}");
        // The declaration appears once, not once as a reference and once as a declaration.
        let declarations = with
            .iter()
            .filter(|location| location.range.start.line == declaration_line)
            .count();
        assert_eq!(declarations, 1, "{with:?}");
        assert_eq!(with.len(), 3, "{with:?}");
    }

    #[test]
    fn a_failing_analysis_leaves_the_document_out_and_says_so_until_it_changes() {
        let is_failure = |diagnostic: &Diagnostic| {
            diagnostic.code == Some(NumberOrString::String(ANALYSIS_FAILED_CODE.into()))
        };
        let mut server = started();
        let uri = url("file:///lib/main.dart");

        fault::set_analysis(true);
        open(&mut server, &uri, "class A {}\n");
        fault::set_analysis(false);

        assert!(server.diagnostics(&uri).iter().any(is_failure));
        assert!(
            server
                .document_symbols(&uri, symbols_params(&uri))
                .unwrap()
                .is_none()
        );
        assert_eq!(text_of(&server, &uri), "class A {}\n");

        server.did_change(change(&uri, 2, vec![edit((0, 0), (0, 0), "// edited\n")]));

        assert!(!server.diagnostics(&uri).iter().any(is_failure));
        let symbols = server
            .document_symbols(&uri, symbols_params(&uri))
            .unwrap()
            .unwrap();
        assert_eq!(symbols[0].name, "A");
    }

    #[test]
    fn a_document_over_the_size_limit_keeps_its_outline_but_not_navigation() {
        let mut server = started();
        let uri = url("file:///lib/generated.dart");
        let mut text = String::new();
        let mut index = 0;
        while text.len() <= MAX_NAVIGATION_BYTES {
            text.push_str(&format!("class Generated{index} {{}}\n"));
            index += 1;
        }
        let call = text.len();
        text.push_str("void use() { Generated0(); }\n");
        open(&mut server, &uri, &text);

        let at = byte_offset_to_lsp_position(&text, call + "void use() { ".len());
        assert!(server.definition(&uri, at).unwrap().is_none());
        let symbols = server
            .document_symbols(&uri, symbols_params(&uri))
            .unwrap()
            .unwrap();
        assert_eq!(symbols.len(), index + 1);
        let notice = server
            .diagnostics(&uri)
            .into_iter()
            .find(|diagnostic| {
                diagnostic.code == Some(NumberOrString::String(NAVIGATION_DISABLED_CODE.into()))
            })
            .expect("a diagnostic that says why navigation is off");
        assert_eq!(notice.severity, Some(DiagnosticSeverity::INFORMATION));

        // The same call in a document under the limit resolves.
        let small = url("file:///lib/small.dart");
        let small_text = "class Generated0 {}\nvoid use() { Generated0(); }\n";
        open(&mut server, &small, small_text);
        let at = byte_offset_to_lsp_position(small_text, small_text.rfind("Generated0").unwrap());
        assert!(server.definition(&small, at).unwrap().is_some());
        assert!(
            !server
                .diagnostics(&small)
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(NumberOrString::String(NAVIGATION_DISABLED_CODE.into())))
        );
    }

    #[test]
    fn closing_a_document_removes_it_from_navigation() {
        let mut server = started();
        let lib = url("file:///lib/lib.dart");
        let main = url("file:///lib/main.dart");
        open(&mut server, &lib, "class Widget {}\n");
        let main_text = "import 'lib.dart';\nvoid run() { Widget(); }\n";
        open(&mut server, &main, main_text);
        let at = byte_offset_to_lsp_position(main_text, main_text.find("Widget").unwrap());
        assert!(server.definition(&main, at).unwrap().is_some());

        server.did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: lib.clone() },
        });

        assert!(server.definition(&main, at).unwrap().is_none());
        assert!(matches!(
            server.document_symbols(&lib, symbols_params(&lib)),
            Err(LspError::DocumentNotOpen(_))
        ));
    }

    #[test]
    fn the_outline_nests_members_and_selects_names_inside_the_ranges() {
        let mut server = started();
        let uri = url("file:///lib/main.dart");
        let text = "\
class Box {
  final int size;
  Box(this.size);
  int area() {
    var side = size;
    return side * side;
  }
  int get half => size ~/ 2;
}
enum Color { red, green }
int top() => 1;
int get answer => 42;
";
        open(&mut server, &uri, text);

        let symbols = server
            .document_symbols(&uri, symbols_params(&uri))
            .unwrap()
            .unwrap();

        let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(names, ["Box", "Color", "top", "answer"]);
        let kinds: Vec<_> = symbols.iter().map(|symbol| symbol.kind).collect();
        assert_eq!(
            kinds,
            [
                SymbolKind::Class,
                SymbolKind::Enum,
                SymbolKind::Function,
                SymbolKind::Property
            ]
        );
        let members = symbols[0].children.as_ref().unwrap();
        let member_names: Vec<_> = members.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(member_names, ["size", "Box", "area", "half"]);
        let constants = symbols[1].children.as_ref().unwrap();
        assert_eq!(constants[0].kind, SymbolKind::EnumMember);

        fn contains(outer: &Range, inner: &Range) -> bool {
            let key = |position: &Position| (position.line, position.character);
            key(&outer.start) <= key(&inner.start) && key(&inner.end) <= key(&outer.end)
        }
        let lines = LineIndex::new(text);
        let mut pending: Vec<&DocumentSymbol> = symbols.iter().collect();
        while let Some(symbol) = pending.pop() {
            assert!(
                contains(&symbol.range, &symbol.selection_range),
                "{}: selection {:?} outside {:?}",
                symbol.name,
                symbol.selection_range,
                symbol.range
            );
            // The selection is the name itself, not the line or the declaration.
            let start = lines.offset(symbol.selection_range.start).unwrap();
            let end = lines.offset(symbol.selection_range.end).unwrap();
            assert_eq!(
                &text[start..end],
                symbol.name,
                "selection of {}",
                symbol.name
            );
            pending.extend(symbol.children.iter().flatten());
        }
        // Locals are not outline entries.
        let area = &members[2];
        assert!(area.children.is_none());
    }
}
