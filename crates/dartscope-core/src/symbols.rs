//! Symbol queries and their resolutions.

use serde::{Deserialize, Serialize};

use crate::declarations::DartDeclarationKind;
use crate::path::normalize_path;
use crate::span::SourceSpan;

/// One project-level query for a top-level Dart declaration visible from a source library.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartSymbolQuery {
    pub source_path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

impl DartSymbolQuery {
    pub fn new(source_path: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source_path: normalize_path(source_path.into()),
            name: name.into(),
            prefix: None,
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

/// Deterministic result of resolving one top-level declaration query.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartSymbolResolution {
    pub query: DartSymbolQuery,
    pub status: DartSymbolResolutionStatus,
    pub candidates: Vec<DartSymbolCandidate>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartSymbolResolutionStatus {
    Resolved,
    Missing,
    Ambiguous,
    NotVisible,
    ConditionalEnvironmentRequired,
    SourceFileMissing,
}

/// One declaration candidate retained as namespace-resolution evidence.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartSymbolCandidate {
    pub name: String,
    pub kind: DartDeclarationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    pub declaration_path: String,
    pub declaration_span: SourceSpan,
    pub basis: DartSymbolResolutionBasis,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartSymbolResolutionBasis {
    SameFile,
    SameLibrary,
    DirectImport,
    ReExport,
    NotVisible,
}
