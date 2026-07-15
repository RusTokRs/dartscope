//! Conservative Dart file and project analysis.

mod analysis;
mod backend;
mod declarations;
mod flutter_hints;
mod graphql;
mod lexical;
mod namespace;
mod pubspec;
mod pubspec_configuration;
mod pubspec_source;
mod source_lines;

pub use analysis::{analyze_file, analyze_project};
pub use backend::{
    analyze_project_with_parser, DartLanguageVersionCoverage, DartParser, DartParserCapability,
    DartParserCapabilityStatus, DartParserCapabilitySupport, DartParserMetadata,
    HeuristicDartParser,
};
pub use pubspec::parse_pubspec;
pub use pubspec_configuration::{
    parse_pubspec_configuration, PubspecConfigurationAnalysis, PubspecEnvironmentConstraint,
    PubspecFlutterAsset, PubspecFlutterConfiguration, PubspecFlutterFont,
    PubspecFlutterFontFamily,
};
pub use pubspec_source::{
    parse_normalized_dependency_source, PubspecDependencySource, PubspecDependencySourceExt,
    PubspecDependencySourceField,
};

#[cfg(test)]
mod tests;
