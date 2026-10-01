//! Dart-embedded GraphQL operations, their uses, and the contract between them.

use serde::{Deserialize, Serialize};

use crate::references::DartEnclosingSymbol;
use crate::span::SourceSpan;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartGraphqlOperation {
    pub constant_name: String,
    pub operation_type: DartGraphqlOperationType,
    pub operation_name: Option<String>,
    pub variable_names: Vec<String>,
    pub root_fields: Vec<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartGraphqlOperationUse {
    pub constant_name: String,
    pub client_call: DartGraphqlClientCall,
    pub variable_names: Vec<String>,
    pub enclosing_callable: Option<String>,
    pub enclosing_symbol: Option<DartEnclosingSymbol>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartGraphqlContractAnalysis {
    pub bindings: Vec<DartGraphqlOperationBinding>,
    pub unresolved_uses: Vec<DartGraphqlUnresolvedOperationUse>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartGraphqlOperationBinding {
    pub constant_name: String,
    pub resolution_basis: DartGraphqlBindingResolution,
    pub operation_name: Option<String>,
    pub operation_type: DartGraphqlOperationType,
    pub client_call: DartGraphqlClientCall,
    pub call_compatibility: DartGraphqlCallCompatibility,
    pub declared_variable_names: Vec<String>,
    pub supplied_variable_names: Vec<String>,
    pub missing_variable_names: Vec<String>,
    pub unexpected_variable_names: Vec<String>,
    pub variable_compatibility: DartGraphqlVariableCompatibility,
    pub operation_path: String,
    pub operation_span: SourceSpan,
    pub use_path: String,
    pub use_span: SourceSpan,
    pub enclosing_symbol: Option<DartEnclosingSymbol>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlBindingResolution {
    SameFile,
    SameLibrary,
    DirectImport,
    ReExport,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartGraphqlUnresolvedOperationUse {
    pub constant_name: String,
    pub reason: DartGraphqlUnresolvedReason,
    pub use_path: String,
    pub use_span: SourceSpan,
    pub candidate_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlUnresolvedReason {
    MissingDeclaration,
    AmbiguousDeclaration,
    NotVisibleDeclaration,
    ConditionalEnvironmentRequired,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlCallCompatibility {
    Match,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlVariableCompatibility {
    Match,
    Mismatch,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlClientCall {
    Query,
    Mutation,
    Subscription,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartGraphqlOperationType {
    Query,
    Mutation,
    Subscription,
}
