//! The URI graph of a project and the ownership links between libraries and their parts.

use serde::{Deserialize, Serialize};

use crate::span::SourceSpan;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartUriGraph {
    pub references: Vec<DartUriReference>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartPartLinkAnalysis {
    pub links: Vec<DartPartLink>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPartLink {
    pub owner_path: String,
    pub part_uri: String,
    pub part_path: Option<String>,
    pub declared_owner: Option<String>,
    pub status: DartPartLinkStatus,
    pub part_span: SourceSpan,
    pub part_of_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartPartLinkStatus {
    Matched,
    MissingTarget,
    UnresolvedTarget,
    MissingPartOf,
    DifferentLibrary,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartUriReference {
    pub source_path: String,
    pub source_span: SourceSpan,
    pub uri: String,
    pub condition: Option<String>,
    pub kind: DartUriReferenceKind,
    pub resolution: DartUriResolution,
    pub target_path: Option<String>,
    pub target_uri: Option<String>,
    pub candidate_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartUriReferenceKind {
    Import,
    Export,
    Part,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartUriResolution {
    Resolved,
    ResolvedExternal,
    External,
    MissingTarget,
    UnindexedPackage,
    AmbiguousPackage,
    UnsupportedScheme,
    InvalidConfiguration,
    InvalidUri,
}
