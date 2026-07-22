//! Dart file, project, and structured pubspec analysis.

mod analysis;
mod backend;
mod declaration_inventory;
mod declarations;
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub mod fuzzing;
mod graphql;
mod identifier_references;
mod invocations;
mod lexical;
mod lexical_bindings;
mod lexical_reads;
mod lexical_regions;
mod lexical_writes;
mod member_reference_syntax;
mod member_references;
mod namespace;
mod operator_references;
mod project_input;
mod property_references;
#[path = "pubspec_analysis.rs"]
mod pubspec;
mod pubspec_backend;
#[path = "pubspec_configuration_analysis.rs"]
mod pubspec_configuration;
mod pubspec_source;
mod pubspec_syntax;
#[cfg(test)]
mod pubspec_yaml_contract;
#[allow(dead_code)]
mod pubspec_yaml_marked;
mod pubspec_yaml_marked_analysis;
mod pubspec_yaml_marked_configuration;
#[allow(dead_code)]
mod pubspec_yaml_marked_dependencies;
mod pubspec_yaml_subset;
mod source_lines;

use dartscope_core::{DartProjectAnalysis, DartProjectInput, DartProjectReferenceAnalysis};

pub use analysis::{analyze_file, analyze_file_with_references};
pub use backend::{
    DartLanguageVersionCoverage, DartParser, DartParserCapability, DartParserCapabilityStatus,
    DartParserCapabilitySupport, DartParserMetadata, HeuristicDartParser,
};
pub use dartscope_core::pubspec::PubspecConfiguration;
pub use pubspec::parse_pubspec;
pub use pubspec_configuration::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetSelectorPolicy,
    PubspecFlutterAssetTransformer, PubspecFlutterConfiguration, PubspecFlutterFont,
    PubspecFlutterFontFamily, parse_pubspec_configuration,
};
pub use pubspec_source::{
    PubspecDependencySource, PubspecDependencySourceExt, PubspecDependencySourceField,
    parse_normalized_dependency_source,
};

/// Analyzes a project after validating normalized input-path identity.
pub fn analyze_project(input: DartProjectInput) -> DartProjectAnalysis {
    let (input, diagnostics) = project_input::prepare_project_input(input);
    let mut analysis = analysis::analyze_project(input);
    project_input::append_project_diagnostics(&mut analysis, diagnostics);
    analysis
}

/// Analyzes a project and opt-in conservative reference facts after input validation.
pub fn analyze_project_with_references(input: DartProjectInput) -> DartProjectReferenceAnalysis {
    let (input, diagnostics) = project_input::prepare_project_input(input);
    let mut analysis = analysis::analyze_project_with_references(input);
    project_input::append_project_diagnostics(&mut analysis.project, diagnostics);
    analysis
}

/// Analyzes a project through a caller-provided parser after input validation.
pub fn analyze_project_with_parser(
    parser: &dyn DartParser,
    input: DartProjectInput,
) -> DartProjectAnalysis {
    let (input, diagnostics) = project_input::prepare_project_input(input);
    let mut analysis = backend::analyze_project_with_parser(parser, input);
    project_input::append_project_diagnostics(&mut analysis, diagnostics);
    analysis
}

#[cfg(test)]
mod tests;
