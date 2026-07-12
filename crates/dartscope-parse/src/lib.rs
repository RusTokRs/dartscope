//! Conservative Dart file and project analysis.

mod analysis;
mod backend;
mod declarations;
mod flutter_hints;
mod graphql;
mod lexical;
mod namespace;
mod pubspec;
mod source_lines;

pub use analysis::{analyze_file, analyze_project};
pub use backend::{
    analyze_project_with_parser, DartLanguageVersionCoverage, DartParser, DartParserCapability,
    DartParserCapabilityStatus, DartParserCapabilitySupport, DartParserMetadata,
    HeuristicDartParser,
};
pub use pubspec::parse_pubspec;

#[cfg(test)]
mod tests;
