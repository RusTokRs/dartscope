use dartscope_core::pubspec::PubspecConfigurationAnalysis;
use dartscope_core::{PubspecAnalysis, PubspecInput};

pub(crate) fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    crate::pubspec_yaml_marked_analysis::parse_pubspec(input)
}

pub(crate) fn parse_pubspec_configuration(input: PubspecInput) -> PubspecConfigurationAnalysis {
    let prepared = crate::pubspec_syntax::prepare_pubspec_source(&input.source);
    let marked_source = crate::pubspec_yaml_marked_analysis::sanitize_bare_wildcards(
        &prepared.source,
        &prepared.syntax,
    );
    let mut analysis = crate::pubspec_yaml_marked_configuration::parse_pubspec_configuration(
        PubspecInput::new(input.path, marked_source),
    );
    crate::pubspec_syntax::append_common_syntax_diagnostics(
        &mut analysis.diagnostics,
        &analysis.path,
        &prepared.syntax,
    );
    analysis
}
