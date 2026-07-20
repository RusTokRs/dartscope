use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod pubspec;

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
pub struct PackageConfigInput {
    pub path: String,
    pub source: String,
}

impl PackageConfigInput {
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
    #[serde(default)]
    pub package_configs: Vec<PackageConfigInput>,
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
            package_configs: Vec::new(),
        }
    }

    pub fn with_package_configs(mut self, package_configs: Vec<PackageConfigInput>) -> Self {
        self.package_configs = package_configs;
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartFileAnalysis {
    pub path: String,
    pub language: DartFileLanguage,
    pub library: Option<DartLibraryDirective>,
    pub imports: Vec<DartImport>,
    pub exports: Vec<DartExport>,
    pub parts: Vec<DartPart>,
    pub part_of: Option<DartPartOf>,
    pub declarations: Vec<DartDeclaration>,
    pub string_constants: Vec<DartStringConstant>,
    pub graphql_operations: Vec<DartGraphqlOperation>,
    pub graphql_operation_uses: Vec<DartGraphqlOperationUse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<DartInvocation>,
    /// Compatibility projection populated only by Flutter-aware composition.
    ///
    /// Pure Dart parser backends leave this field empty. It remains serialized in the
    /// v1 model so older consumers can migrate to `dartscope-flutter` without a breaking
    /// schema change.
    pub flutter: FlutterFileHints,
    pub diagnostics: Vec<DartDiagnostic>,
}

impl DartFileAnalysis {
    pub fn empty(path: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            language: DartFileLanguage::Dart,
            library: None,
            imports: Vec::new(),
            exports: Vec::new(),
            parts: Vec::new(),
            part_of: None,
            declarations: Vec::new(),
            string_constants: Vec::new(),
            graphql_operations: Vec::new(),
            graphql_operation_uses: Vec::new(),
            invocations: Vec::new(),
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
    pub package_configs: Vec<PackageConfigAnalysis>,
    pub summary: DartProjectSummary,
    pub diagnostics: Vec<DartDiagnostic>,
}

/// Opt-in file analysis paired with conservative identifier-reference facts.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartFileReferenceAnalysis {
    pub file: DartFileAnalysis,
    pub references: Vec<DartIdentifierReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<DartLexicalBinding>,
}

/// Opt-in project analysis paired with conservative identifier-reference facts.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartProjectReferenceAnalysis {
    pub project: DartProjectAnalysis,
    pub references: Vec<DartIdentifierReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<DartLexicalBinding>,
}

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
    TypeAnnotation,
    ParameterType,
    ReturnType,
    VariableType,
    VariableRead,
    VariableWrite,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartProjectSummary {
    pub dart_files: usize,
    pub pubspecs: usize,
    pub package_configs: usize,
    pub imports: usize,
    pub exports: usize,
    pub parts: usize,
    pub declarations: usize,
    pub string_constants: usize,
    pub graphql_operations: usize,
    pub graphql_operation_uses: usize,
    pub flutter_widgets: usize,
    pub flutter_routes: usize,
    pub flutter_assets: usize,
    pub flutter_localizations: usize,
    pub package_dependencies: usize,
    pub diagnostics: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartFileLanguage {
    Dart,
    Pubspec,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
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
    pub configurations: Vec<DartUriConfiguration>,
    pub is_deferred: bool,
    pub prefix: Option<String>,
    pub combinators: Vec<DartNamespaceCombinator>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartExport {
    pub uri: String,
    pub configurations: Vec<DartUriConfiguration>,
    pub combinators: Vec<DartNamespaceCombinator>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartUriConfiguration {
    pub condition: String,
    pub uri: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartCompilationEnvironment {
    pub entries: Vec<DartCompilationEnvironmentEntry>,
}

impl DartCompilationEnvironment {
    pub fn new(entries: Vec<DartCompilationEnvironmentEntry>) -> Self {
        Self { entries }
    }

    pub fn from_pairs(
        pairs: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            entries: pairs
                .into_iter()
                .map(|(key, value)| DartCompilationEnvironmentEntry {
                    key: key.into(),
                    value: value.into(),
                })
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartCompilationEnvironmentEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartNamespaceCombinator {
    pub kind: DartNamespaceCombinatorKind,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartNamespaceCombinatorKind {
    Show,
    Hide,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPart {
    pub uri: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPartOf {
    pub library: String,
    pub kind: DartPartOfKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLibraryDirective {
    pub name: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartPartOfKind {
    Uri,
    LibraryName,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartDeclaration {
    pub name: String,
    pub kind: DartDeclarationKind,
    pub span: SourceSpan,
    pub extends: Option<String>,
    pub mixes_in: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_symbol_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_span: Option<SourceSpan>,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartGraphqlContractAnalysis {
    pub bindings: Vec<DartGraphqlOperationBinding>,
    pub unresolved_uses: Vec<DartGraphqlUnresolvedOperationUse>,
}

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

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartDeclarationKind {
    Class,
    Mixin,
    Enum,
    Extension,
    ExtensionType,
    Typedef,
    Function,
    Variable,
    Method,
    Constructor,
    Field,
    Getter,
    Setter,
    Operator,
    LocalVariable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartInvocation {
    /// Normalized dotted target such as `Image.asset` or `DefaultAssetBundle.of.loadString`.
    pub target: String,
    pub arguments: Vec<DartInvocationArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_members: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_symbol_id: Option<String>,
    /// Exact invocation expression span.
    pub span: SourceSpan,
    /// Complete source-line evidence retained for compatibility projections.
    pub source_line_span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartInvocationArgument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Original source expression with surrounding whitespace removed.
    pub expression: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_entries: Vec<DartMapEntry>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartMapEntry {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_key: Option<String>,
    pub value: String,
    pub span: SourceSpan,
    pub source_line_span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterFileHints {
    pub imports_flutter: bool,
    pub widgets: Vec<FlutterWidgetHint>,
    pub routes: Vec<FlutterRouteHint>,
    pub assets: Vec<FlutterAssetHint>,
    pub localizations: Vec<FlutterLocalizationHint>,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterAssetHint {
    pub path: String,
    pub source: FlutterAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Non-literal `package:` expression when exact package identity is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_expression: Option<String>,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterAssetSource {
    ImageAsset,
    AssetImage,
    RootBundleLoadString,
    DefaultAssetBundleLoadString,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterLocalizationHint {
    pub key: String,
    pub source: FlutterLocalizationSource,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterLocalizationSource {
    AppLocalizationsOf,
    GeneratedLocalizationsOf,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

impl DartDiagnostic {
    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            path: None,
            code: code.into(),
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            span,
            confidence: None,
        }
    }

    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            path: None,
            code: code.into(),
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            span,
            confidence: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(normalize_path(path.into()));
        self
    }

    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
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
pub struct PackageConfigAnalysis {
    pub path: String,
    pub config_version: Option<u64>,
    pub packages: Vec<DartPackageConfigEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator_version: Option<String>,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPackageConfigEntry {
    pub name: String,
    pub root_uri: String,
    pub package_uri: Option<String>,
    pub language_version: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartResolvedPackageUri {
    pub package_name: String,
    pub resolved_uri: String,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecAnalysis {
    pub path: String,
    pub package_name: Option<String>,
    pub dependencies: Vec<PubspecDependency>,
    #[serde(default)]
    pub configuration: pubspec::PubspecConfiguration,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecDependency {
    pub name: String,
    pub section: PubspecDependencySection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<pubspec::PubspecDependencySource>,
    pub version_or_source: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PubspecDependencySection {
    #[default]
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn older_flutter_asset_hints_deserialize_without_package_metadata() {
        let hint: FlutterAssetHint = serde_json::from_str(
            r#"{
                "path": "assets/logo.png",
                "source": "image_asset",
                "confidence": "high",
                "span": {
                    "byte_start": 0,
                    "byte_end": 10,
                    "start_line": 1,
                    "start_column": 1,
                    "end_line": 1,
                    "end_column": 11
                }
            }"#,
        )
        .expect("legacy Flutter asset hint");

        assert_eq!(hint.package, None);
        assert_eq!(hint.package_expression, None);
    }

    #[test]
    fn older_diagnostics_deserialize_without_confidence() {
        let diagnostic: DartDiagnostic = serde_json::from_str(
            r#"{"path":"lib/main.dart","code":"example","severity":"warning","message":"example","span":null}"#,
        )
        .expect("legacy diagnostic");

        assert_eq!(diagnostic.confidence, None);
        assert!(
            !serde_json::to_string(&diagnostic)
                .expect("serialize diagnostic")
                .contains("confidence")
        );
    }
}
