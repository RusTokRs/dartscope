//! The result containers of analyzing one file or one project, and the project summary.

use serde::{Deserialize, Serialize};

use crate::declarations::{DartDeclaration, DartInvocation, DartStringConstant};
use crate::diagnostic::DartDiagnostic;
use crate::directives::{DartExport, DartImport, DartLibraryDirective, DartPart, DartPartOf};
use crate::flutter::FlutterFileHints;
use crate::graphql::{DartGraphqlOperation, DartGraphqlOperationUse};
use crate::package::{PackageConfigAnalysis, PubspecAnalysis};
use crate::path::normalize_path;
use crate::references::{DartIdentifierReference, DartLexicalBinding};

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
