//! Conservative Dart file and project analysis.

mod analysis;
mod backend;
mod declarations;
mod flutter_hints;
mod graphql;
mod lexical;
mod namespace;
#[path = "pubspec_analysis.rs"]
mod pubspec;
#[path = "pubspec.rs"]
mod pubspec_dependencies;
mod pubspec_assets;
#[path = "pubspec_configuration_analysis.rs"]
mod pubspec_configuration;
#[path = "pubspec_configuration.rs"]
mod pubspec_configuration_legacy;
mod pubspec_source;
mod pubspec_syntax;
#[allow(dead_code)]
mod pubspec_yaml_marked;
#[allow(dead_code)]
mod pubspec_yaml_marked_configuration;
mod pubspec_yaml_subset;
mod source_lines;

pub use analysis::{analyze_file, analyze_project};
pub use backend::{
    analyze_project_with_parser, DartLanguageVersionCoverage, DartParser, DartParserCapability,
    DartParserCapabilityStatus, DartParserCapabilitySupport, DartParserMetadata,
    HeuristicDartParser,
};
pub use dartscope_core::pubspec::PubspecConfiguration;
pub use pubspec::parse_pubspec;
pub use pubspec_configuration::{
    parse_pubspec_configuration, PubspecConfigurationAnalysis, PubspecEnvironmentConstraint,
    PubspecFlutterAsset, PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer,
    PubspecFlutterConfiguration, PubspecFlutterFont, PubspecFlutterFontFamily,
};
pub use pubspec_source::{
    parse_normalized_dependency_source, PubspecDependencySource, PubspecDependencySourceExt,
    PubspecDependencySourceField,
};

#[cfg(test)]
mod tests;
