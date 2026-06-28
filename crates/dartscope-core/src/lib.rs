use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartFileInput {
    pub path: String,
    pub source: String,
}

impl DartFileInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecInput {
    pub path: String,
    pub source: String,
}

impl PubspecInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartProjectInput {
    pub root: String,
    pub files: Vec<DartFileInput>,
    pub pubspecs: Vec<PubspecInput>,
}

impl DartProjectInput {
    pub fn new(
        root: impl Into<String>,
        files: Vec<DartFileInput>,
        pubspecs: Vec<PubspecInput>,
    ) -> Self {
        Self {
            root: normalize_path(root.into()),
            files,
            pubspecs,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartFileAnalysis {
    pub path: String,
    pub language: DartFileLanguage,
    pub imports: Vec<DartImport>,
    pub exports: Vec<DartExport>,
    pub parts: Vec<DartPart>,
    pub part_of: Option<DartPartOf>,
    pub declarations: Vec<DartDeclaration>,
    pub string_constants: Vec<DartStringConstant>,
    pub graphql_operations: Vec<DartGraphqlOperation>,
    pub graphql_operation_uses: Vec<DartGraphqlOperationUse>,
    pub flutter: FlutterFileHints,
    pub diagnostics: Vec<DartDiagnostic>,
}

impl DartFileAnalysis {
    pub fn empty(path: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            language: DartFileLanguage::Dart,
            imports: Vec::new(),
            exports: Vec::new(),
            parts: Vec::new(),
            part_of: None,
            declarations: Vec::new(),
            string_constants: Vec::new(),
            graphql_operations: Vec::new(),
            graphql_operation_uses: Vec::new(),
            flutter: FlutterFileHints::default(),
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartProjectAnalysis {
    pub root: String,
    pub files: Vec<DartFileAnalysis>,
    pub pubspecs: Vec<PubspecAnalysis>,
    pub summary: DartProjectSummary,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartProjectSummary {
    pub dart_files: usize,
    pub pubspecs: usize,
    pub imports: usize,
    pub exports: usize,
    pub parts: usize,
    pub declarations: usize,
    pub string_constants: usize,
    pub graphql_operations: usize,
    pub graphql_operation_uses: usize,
    pub flutter_widgets: usize,
    pub flutter_routes: usize,
    pub package_dependencies: usize,
    pub diagnostics: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartFileLanguage {
    Dart,
    Pubspec,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SourceSpan {
    pub fn line(line_number: usize, byte_start: usize, text: &str) -> Self {
        Self {
            byte_start,
            byte_end: byte_start + text.len(),
            start_line: line_number,
            start_column: 1,
            end_line: line_number,
            end_column: text.chars().count() + 1,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartImport {
    pub uri: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartExport {
    pub uri: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPart {
    pub uri: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPartOf {
    pub library: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartDeclaration {
    pub name: String,
    pub kind: DartDeclarationKind,
    pub span: SourceSpan,
    pub extends: Option<String>,
    pub mixes_in: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartStringConstant {
    pub name: String,
    pub value: String,
    pub span: SourceSpan,
}

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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartDeclarationKind {
    Class,
    Mixin,
    Enum,
    Extension,
    Typedef,
    Function,
    Variable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterFileHints {
    pub imports_flutter: bool,
    pub widgets: Vec<FlutterWidgetHint>,
    pub routes: Vec<FlutterRouteHint>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterWidgetHint {
    pub class_name: String,
    pub base_class: String,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterRouteHint {
    pub constructor: String,
    pub path: String,
    pub path_kind: FlutterRoutePathKind,
    pub resolved_path: Option<String>,
    pub name: Option<String>,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterRoutePathKind {
    Literal,
    Expression,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartDiagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl DartDiagnostic {
    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecAnalysis {
    pub path: String,
    pub package_name: Option<String>,
    pub dependencies: Vec<PubspecDependency>,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecDependency {
    pub name: String,
    pub section: PubspecDependencySection,
    pub version_or_source: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PubspecDependencySection {
    Dependencies,
    DevDependencies,
    DependencyOverrides,
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum DartScopeError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("JSON error: {0}")]
    Json(String),
}

pub fn normalize_path(path: String) -> String {
    path.replace('\\', "/")
}
