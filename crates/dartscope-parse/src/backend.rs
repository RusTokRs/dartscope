use dartscope_core::{DartFileAnalysis, DartFileInput, DartProjectAnalysis, DartProjectInput};

use crate::analysis::{analyze_file_heuristic, analyze_project_with_backend};

/// A source-only Dart parser backend.
///
/// Backends receive already-loaded source and return normalized DartScope facts. They must not
/// require consumers to depend on a backend-specific AST or perform filesystem I/O.
pub trait DartParser {
    /// Identifies the backend and the facts it can provide.
    fn metadata(&self) -> DartParserMetadata;

    /// Analyzes one Dart source file.
    fn analyze_file(&self, input: DartFileInput) -> DartFileAnalysis;
}

/// Metadata that lets consumers distinguish unavailable facts from empty findings.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartParserMetadata {
    pub name: String,
    pub capabilities: Vec<DartParserCapabilityStatus>,
    pub language_version: DartLanguageVersionCoverage,
}

impl DartParserMetadata {
    pub fn support_for(&self, capability: DartParserCapability) -> DartParserCapabilitySupport {
        self.capabilities
            .iter()
            .find(|status| status.capability == capability)
            .map(|status| status.support)
            .unwrap_or(DartParserCapabilitySupport::Unsupported)
    }
}

/// A normalized category of parser facts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DartParserCapability {
    Directives,
    Declarations,
    Members,
    Recovery,
}

/// Whether a backend can provide a normalized category of facts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DartParserCapabilitySupport {
    Supported,
    Unsupported,
}

/// One explicit capability declaration for a parser backend.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DartParserCapabilityStatus {
    pub capability: DartParserCapability,
    pub support: DartParserCapabilitySupport,
}

/// Declares how completely a backend covers Dart language versions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DartLanguageVersionCoverage {
    /// The backend supports only documented syntax slices, not a complete language version.
    Partial {
        minimum: Option<String>,
        maximum: Option<String>,
    },
    /// The backend does not make a language-version claim.
    Unspecified,
}

/// The built-in conservative, line-oriented parser backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicDartParser;

impl DartParser for HeuristicDartParser {
    fn metadata(&self) -> DartParserMetadata {
        DartParserMetadata {
            name: "heuristic".to_string(),
            capabilities: vec![
                DartParserCapabilityStatus {
                    capability: DartParserCapability::Directives,
                    support: DartParserCapabilitySupport::Supported,
                },
                DartParserCapabilityStatus {
                    capability: DartParserCapability::Declarations,
                    support: DartParserCapabilitySupport::Supported,
                },
                DartParserCapabilityStatus {
                    capability: DartParserCapability::Members,
                    support: DartParserCapabilitySupport::Unsupported,
                },
                DartParserCapabilityStatus {
                    capability: DartParserCapability::Recovery,
                    support: DartParserCapabilitySupport::Supported,
                },
            ],
            language_version: DartLanguageVersionCoverage::Partial {
                minimum: None,
                maximum: None,
            },
        }
    }

    fn analyze_file(&self, input: DartFileInput) -> DartFileAnalysis {
        analyze_file_heuristic(input)
    }
}

/// Analyzes a project through a caller-provided parser backend.
pub fn analyze_project_with_parser(
    parser: &dyn DartParser,
    input: DartProjectInput,
) -> DartProjectAnalysis {
    analyze_project_with_backend(parser, input)
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_project_with_parser, DartLanguageVersionCoverage, DartParser, DartParserCapability,
        DartParserCapabilityStatus, DartParserCapabilitySupport, DartParserMetadata,
        HeuristicDartParser,
    };
    use crate::analyze_file;
    use dartscope_core::{DartFileAnalysis, DartFileInput, DartProjectInput};

    #[test]
    fn default_backend_keeps_convenience_analysis_behavior() {
        let input = DartFileInput::new("lib/main.dart", "class App {}");

        assert_eq!(
            HeuristicDartParser.analyze_file(input.clone()),
            analyze_file(input)
        );
    }

    #[test]
    fn heuristic_metadata_makes_unsupported_members_explicit() {
        let metadata = HeuristicDartParser.metadata();

        assert_eq!(metadata.name, "heuristic");
        assert_eq!(
            metadata.support_for(DartParserCapability::Directives),
            DartParserCapabilitySupport::Supported
        );
        assert_eq!(
            metadata.support_for(DartParserCapability::Members),
            DartParserCapabilitySupport::Unsupported
        );
        assert!(matches!(
            metadata.language_version,
            DartLanguageVersionCoverage::Partial { .. }
        ));
    }

    #[test]
    fn callers_can_inject_a_source_only_backend() {
        let analysis = analyze_project_with_parser(
            &EmptyParser,
            DartProjectInput::new(
                "demo",
                vec![DartFileInput::new("lib/custom.dart", "class Ignored {}")],
                Vec::new(),
            ),
        );

        assert_eq!(analysis.files.len(), 1);
        assert_eq!(analysis.files[0].path, "lib/custom.dart");
        assert!(analysis.files[0].declarations.is_empty());
    }

    struct EmptyParser;

    impl DartParser for EmptyParser {
        fn metadata(&self) -> DartParserMetadata {
            DartParserMetadata {
                name: "empty-test-parser".to_string(),
                capabilities: vec![DartParserCapabilityStatus {
                    capability: DartParserCapability::Declarations,
                    support: DartParserCapabilitySupport::Unsupported,
                }],
                language_version: DartLanguageVersionCoverage::Unspecified,
            }
        }

        fn analyze_file(&self, input: DartFileInput) -> DartFileAnalysis {
            DartFileAnalysis::empty(input.path)
        }
    }
}
