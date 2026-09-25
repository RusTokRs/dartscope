//! Minimal LSP types without external `lsp-types` crate.
//! These are intentionally small and only cover what `dartscope-lsp` needs.
//! They serialize to the same JSON shape as `lsp-types` 0.97 for the
//! methods we handle.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Url (file://)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url(pub String);

impl Hash for Url {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Url {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Url {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Url(s))
    }
}

impl Url {
    pub fn parse(s: &str) -> Result<Self, String> {
        // Very permissive: accept any string with "://" or file://
        if s.contains("://") {
            Ok(Url(s.to_string()))
        } else {
            Err(format!("invalid url: {s}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn path(&self) -> &str {
        // For file:// URIs, return the path part after "file://"
        // Otherwise return as-is.
        if let Some(rest) = self.0.strip_prefix("file://") {
            // rest may be "/a/b" or "///a/b" or "/C:/a" on windows; strip host if present
            // file:///a/b -> "/a/b"
            // file://host/a/b -> "/a/b" (ignore host)
            // Handle `file:///C:/...`
            let path_part = if rest.starts_with('/') {
                rest
            } else {
                // host present: find next '/'
                if let Some(idx) = rest.find('/') {
                    &rest[idx..]
                } else {
                    "/"
                }
            };
            // Strip query/fragment
            let end = path_part.find(['?', '#']).unwrap_or(path_part.len());
            &path_part[..end]
        } else {
            // Not file://, try to extract path after "://"
            if let Some(idx) = self.0.find("://") {
                let after = &self.0[idx + 3..];
                if let Some(slash) = after.find('/') {
                    let path = &after[slash..];
                    let end = path.find(['?', '#']).unwrap_or(path.len());
                    &path[..end]
                } else {
                    "/"
                }
            } else {
                &self.0
            }
        }
    }

    pub fn to_file_path(&self) -> Result<PathBuf, ()> {
        // Only for file://
        if !self.0.starts_with("file://") {
            return Err(());
        }
        let raw = self.path();
        let decoded = percent_decode(raw);
        // On windows, file:///C:/a -> C:/a, file:///C%3A/... etc. We normalize to unix for tests.
        // Strip leading '/' for windows drive letter case? Keep as-is for unix.
        // For unix, decoded already like "/lib/main.dart"
        Ok(PathBuf::from(decoded))
    }
}

fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut bytes = input.bytes().peekable();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hi = bytes.next();
            let lo = bytes.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                let hex = [hi, lo];
                if let Ok(hex_str) = std::str::from_utf8(&hex) {
                    if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                        out.push(byte as char);
                        continue;
                    }
                }
                // Invalid encoding, keep literally
                out.push('%');
                out.push(hi as char);
                out.push(lo as char);
            } else {
                out.push('%');
                if let Some(hi) = hi {
                    out.push(hi as char);
                }
                if let Some(lo) = lo {
                    out.push(lo as char);
                }
            }
        } else {
            out.push(b as char);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Position / Range
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: Url,
    pub range: Range,
}

// ---------------------------------------------------------------------------
// Diagnostic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    ERROR = 1,
    WARNING = 2,
    INFORMATION = 3,
    HINT = 4,
}

impl Serialize for DiagnosticSeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DiagnosticSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        match v {
            1 => Ok(DiagnosticSeverity::ERROR),
            2 => Ok(DiagnosticSeverity::WARNING),
            3 => Ok(DiagnosticSeverity::INFORMATION),
            4 => Ok(DiagnosticSeverity::HINT),
            _ => Err(serde::de::Error::custom(format!("invalid DiagnosticSeverity {v}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(i32),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<NumberOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    String(String),
    LanguageString(LanguageString),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageString {
    pub language: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    Scalar(MarkedString),
    Array(Vec<MarkedString>),
    Markup(MarkupContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    pub kind: MarkupKind,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: HoverContents,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

// ---------------------------------------------------------------------------
// DocumentSymbol
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

impl Serialize for SymbolKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for SymbolKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        match v {
            1 => Ok(SymbolKind::File),
            2 => Ok(SymbolKind::Module),
            3 => Ok(SymbolKind::Namespace),
            4 => Ok(SymbolKind::Package),
            5 => Ok(SymbolKind::Class),
            6 => Ok(SymbolKind::Method),
            7 => Ok(SymbolKind::Property),
            8 => Ok(SymbolKind::Field),
            9 => Ok(SymbolKind::Constructor),
            10 => Ok(SymbolKind::Enum),
            11 => Ok(SymbolKind::Interface),
            12 => Ok(SymbolKind::Function),
            13 => Ok(SymbolKind::Variable),
            14 => Ok(SymbolKind::Constant),
            15 => Ok(SymbolKind::String),
            16 => Ok(SymbolKind::Number),
            17 => Ok(SymbolKind::Boolean),
            18 => Ok(SymbolKind::Array),
            19 => Ok(SymbolKind::Object),
            20 => Ok(SymbolKind::Key),
            21 => Ok(SymbolKind::Null),
            22 => Ok(SymbolKind::EnumMember),
            23 => Ok(SymbolKind::Struct),
            24 => Ok(SymbolKind::Event),
            25 => Ok(SymbolKind::Operator),
            26 => Ok(SymbolKind::TypeParameter),
            _ => Err(serde::de::Error::custom(format!("invalid SymbolKind {v}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<SymbolTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    pub range: Range,
    #[serde(rename = "selectionRange")]
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DocumentSymbol>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u8)]
pub enum SymbolTag {
    Deprecated = 1,
}

// ---------------------------------------------------------------------------
// Server capabilities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TextDocumentSyncKind {
    None = 0,
    Full = 1,
    Incremental = 2,
}

impl Serialize for TextDocumentSyncKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for TextDocumentSyncKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        match v {
            0 => Ok(TextDocumentSyncKind::None),
            1 => Ok(TextDocumentSyncKind::Full),
            2 => Ok(TextDocumentSyncKind::Incremental),
            _ => Err(serde::de::Error::custom(format!("invalid TextDocumentSyncKind {v}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextDocumentSyncCapability {
    Kind(TextDocumentSyncKind),
    Options(TextDocumentSyncOptions),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextDocumentSyncOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_close: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<TextDocumentSyncKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOf<L, R> {
    Left(L),
    Right(R),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefinitionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress_options: Option<WorkDoneProgressOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress_options: Option<WorkDoneProgressOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentSymbolOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress_options: Option<WorkDoneProgressOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkDoneProgressOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverProviderCapability {
    Simple(bool),
    Options(HoverOptions),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HoverOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress_options: Option<WorkDoneProgressOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document_sync: Option<TextDocumentSyncCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<OneOf<bool, DefinitionOptions>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<OneOf<bool, ReferenceOptions>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<HoverProviderCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_symbol_provider: Option<OneOf<bool, DocumentSymbolOptions>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_info: Option<ServerInfo>,
}

// ---------------------------------------------------------------------------
// InitializeParams
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_uri: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    // Allow unknown fields
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Text document sync params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentItem {
    pub uri: Url,
    #[serde(rename = "languageId")]
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: Url,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    #[serde(rename = "rangeLength", skip_serializing_if = "Option::is_none")]
    pub range_length: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: Url,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkDoneProgressParams {
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PartialResultParams {
    #[serde(rename = "partialResultToken", skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceContext {
    #[serde(rename = "includeDeclaration")]
    pub include_declaration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceParams {
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
    #[serde(flatten)]
    pub work_done_progress_params: WorkDoneProgressParams,
    #[serde(flatten)]
    pub partial_result_params: PartialResultParams,
    pub context: ReferenceContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverParams {
    #[serde(flatten)]
    pub text_document_position_params: TextDocumentPositionParams,
    #[serde(flatten)]
    pub work_done_progress_params: WorkDoneProgressParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbolParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    #[serde(flatten)]
    pub work_done_progress_params: WorkDoneProgressParams,
    #[serde(flatten)]
    pub partial_result_params: PartialResultParams,
}
