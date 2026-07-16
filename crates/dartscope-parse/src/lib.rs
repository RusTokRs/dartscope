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
mod pubspec_assets;
#[path = "pubspec_configuration_analysis.rs"]
mod pubspec_configuration;
#[path = "pubspec_configuration.rs"]
mod pubspec_configuration_legacy;
#[path = "pubspec.rs"]
mod pubspec_dependencies;
mod pubspec_source;
mod pubspec_syntax;
#[cfg(test)]
mod pubspec_yaml_backend_parity;
#[allow(dead_code)]
mod pubspec_yaml_marked;
#[allow(dead_code)]
mod pubspec_yaml_marked_analysis;
#[allow(dead_code)]
mod pubspec_yaml_marked_configuration;
#[allow(dead_code)]
mod pubspec_yaml_marked_dependencies;
mod pubspec_yaml_subset;
mod source_lines;

pub use analysis::{analyze_file, analyze_project};
pub use backend::{
    DartLanguageVersionCoverage, DartParser, DartParserCapability, DartParserCapabilityStatus,
    DartParserCapabilitySupport, DartParserMetadata, HeuristicDartParser,
    analyze_project_with_parser,
};
pub use dartscope_core::pubspec::PubspecConfiguration;
pub use pubspec::parse_pubspec;
pub use pubspec_configuration::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily, parse_pubspec_configuration,
};
pub use pubspec_source::{
    PubspecDependencySource, PubspecDependencySourceExt, PubspecDependencySourceField,
    parse_normalized_dependency_source,
};

#[cfg(test)]
mod tests;
