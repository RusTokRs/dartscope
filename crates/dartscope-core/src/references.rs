//! Lexical bindings, identifier references, their resolutions, and the enclosing-symbol evidence.

use serde::{Deserialize, Serialize};

use crate::diagnostic::Confidence;
use crate::path::normalize_path;
use crate::span::SourceSpan;
use crate::symbols::{DartSymbolCandidate, DartSymbolResolutionStatus};

/// One parser-produced lexical binding with an explicit visibility interval.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLexicalBinding {
    pub source_path: String,
    pub name: String,
    pub kind: DartLexicalBindingKind,
    pub symbol_id: String,
    pub enclosing_symbol_id: String,
    pub declaration_span: SourceSpan,
    pub scope_span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartLexicalBindingKind {
    Parameter,
    LocalVariable,
}

/// One syntactically bounded identifier reference discovered by a parser backend.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartIdentifierReference {
    pub source_path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub kind: DartIdentifierReferenceKind,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_symbol_id: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartIdentifierReferenceKind {
    InvocationTarget,
    ConstructorTarget,
    MemberDeclarationInstance,
    MemberDeclarationStatic,
    MemberInvocationInstance,
    MemberInvocationStatic,
    MemberPropertyDeclarationInstance,
    MemberPropertyDeclarationStatic,
    MemberPropertyReadInstance,
    MemberPropertyReadStatic,
    MemberPropertyWriteInstance,
    MemberPropertyWriteStatic,
    MemberOperatorDeclaration,
    MemberOperatorInvocationInstance,
    TypeAnnotation,
    ParameterType,
    ReturnType,
    VariableType,
    VariableRead,
    VariableWrite,
}

/// Batch result for conservative identifier references.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartIdentifierReferenceResolutionAnalysis {
    pub resolutions: Vec<DartIdentifierReferenceResolution>,
}

/// Namespace-resolution result for one parser-produced identifier reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartIdentifierReferenceResolution {
    pub reference: DartIdentifierReference,
    pub status: DartSymbolResolutionStatus,
    pub candidates: Vec<DartSymbolCandidate>,
}

/// One lexical-binding lookup at a source byte offset.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLexicalBindingQuery {
    pub source_path: String,
    pub name: String,
    pub byte_offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_symbol_id: Option<String>,
}

impl DartLexicalBindingQuery {
    pub fn new(
        source_path: impl Into<String>,
        name: impl Into<String>,
        byte_offset: usize,
    ) -> Self {
        Self {
            source_path: normalize_path(source_path.into()),
            name: name.into(),
            byte_offset,
            enclosing_symbol_id: None,
        }
    }

    pub fn with_enclosing_symbol_id(mut self, symbol_id: impl Into<String>) -> Self {
        self.enclosing_symbol_id = Some(symbol_id.into());
        self
    }
}

/// Deterministic result of selecting the most specific parser-produced lexical binding.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLexicalBindingResolution {
    pub query: DartLexicalBindingQuery,
    pub status: DartLexicalBindingResolutionStatus,
    pub candidates: Vec<DartLexicalBinding>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartLexicalBindingResolutionStatus {
    Resolved,
    Missing,
    Ambiguous,
    SourceFileMissing,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartEnclosingSymbol {
    pub name: String,
    pub kind: DartEnclosingSymbolKind,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartEnclosingSymbolKind {
    Callable,
    Variable,
}
