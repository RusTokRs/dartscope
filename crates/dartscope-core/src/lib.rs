//! Normalized analysis models shared by every DartScope crate.
//!
//! The crate holds data and no behavior beyond constructors and small invariants: the inputs a caller
//! hands to an analyzer, the facts an analysis reports (declarations, directives, invocations, GraphQL,
//! Flutter hints, references), the project-level products of indexing, diagnostics, and the pubspec
//! model. Every type serializes with `serde`; the JSON shapes are the contracts documented in
//! `docs/development/json-contracts.md`.
//!
//! The types live in modules by domain and are re-exported from the crate root, which is the public path
//! (`dartscope_core::DartFileAnalysis`). `pubspec` is the only public module.

pub mod pubspec;

mod analysis;
mod declarations;
mod diagnostic;
mod directives;
mod error;
mod flutter;
mod graphql;
mod input;
mod package;
mod path;
mod references;
mod span;
mod symbols;
mod uri_graph;

pub use analysis::{DartFileAnalysis, DartFileLanguage, DartFileReferenceAnalysis, DartProjectAnalysis, DartProjectReferenceAnalysis, DartProjectSummary};
pub use declarations::{DartDeclaration, DartDeclarationKind, DartInvocation, DartInvocationArgument, DartMapEntry, DartStringConstant};
pub use diagnostic::{Confidence, DartDiagnostic, DiagnosticSeverity};
pub use directives::{DartCompilationEnvironment, DartCompilationEnvironmentEntry, DartExport, DartImport, DartLibraryDirective, DartNamespaceCombinator, DartNamespaceCombinatorKind, DartPart, DartPartOf, DartPartOfKind, DartUriConfiguration};
pub use error::DartScopeError;
pub use flutter::{FlutterAssetHint, FlutterAssetSource, FlutterFileHints, FlutterLocalizationHint, FlutterLocalizationSource, FlutterRouteHint, FlutterRoutePathKind, FlutterWidgetHint};
pub use graphql::{DartGraphqlBindingResolution, DartGraphqlCallCompatibility, DartGraphqlClientCall, DartGraphqlContractAnalysis, DartGraphqlOperation, DartGraphqlOperationBinding, DartGraphqlOperationType, DartGraphqlOperationUse, DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason, DartGraphqlVariableCompatibility};
pub use input::{DartFileInput, DartProjectInput, PackageConfigInput, PubspecInput};
pub use package::{DartPackageConfigEntry, DartResolvedPackageUri, PackageConfigAnalysis, PubspecAnalysis, PubspecDependency, PubspecDependencySection};
pub use path::normalize_path;
pub use references::{DartEnclosingSymbol, DartEnclosingSymbolKind, DartIdentifierReference, DartIdentifierReferenceKind, DartIdentifierReferenceResolution, DartIdentifierReferenceResolutionAnalysis, DartLexicalBinding, DartLexicalBindingKind, DartLexicalBindingQuery, DartLexicalBindingResolution, DartLexicalBindingResolutionStatus};
pub use span::SourceSpan;
pub use symbols::{DartSymbolCandidate, DartSymbolQuery, DartSymbolResolution, DartSymbolResolutionBasis, DartSymbolResolutionStatus};
pub use uri_graph::{DartPartLink, DartPartLinkAnalysis, DartPartLinkStatus, DartUriGraph, DartUriReference, DartUriReferenceKind, DartUriResolution};

#[cfg(test)]
mod compatibility_tests;
