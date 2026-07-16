use dartscope_core::pubspec::PubspecConfigurationAnalysis;
use dartscope_core::{PubspecAnalysis, PubspecInput};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum PubspecBackend {
    #[allow(dead_code)]
    Conservative,
    Marked,
}

pub(crate) const DEFAULT_PUBSPEC_BACKEND: PubspecBackend = PubspecBackend::Marked;

pub(crate) fn parse_pubspec_with_backend(
    input: PubspecInput,
    backend: PubspecBackend,
) -> PubspecAnalysis {
    match backend {
        PubspecBackend::Conservative => crate::pubspec::parse_pubspec_conservative(input),
        PubspecBackend::Marked => crate::pubspec_yaml_marked_analysis::parse_pubspec(input),
    }
}

pub(crate) fn parse_pubspec_configuration_with_backend(
    input: PubspecInput,
    backend: PubspecBackend,
) -> PubspecConfigurationAnalysis {
    match backend {
        PubspecBackend::Conservative => {
            crate::pubspec_configuration::parse_pubspec_configuration_conservative(input)
        }
        PubspecBackend::Marked => {
            let prepared = crate::pubspec_syntax::prepare_pubspec_source(&input.source);
            let marked_source = crate::pubspec_yaml_marked_analysis::sanitize_bare_wildcards(
                &prepared.source,
                &prepared.syntax,
            );
            let mut analysis =
                crate::pubspec_yaml_marked_configuration::parse_pubspec_configuration(
                    PubspecInput::new(input.path, marked_source),
                );
            crate::pubspec_syntax::append_common_syntax_diagnostics(
                &mut analysis.diagnostics,
                &analysis.path,
                &prepared.syntax,
            );
            analysis
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marked_backend_is_the_public_default() {
        assert_eq!(DEFAULT_PUBSPEC_BACKEND, PubspecBackend::Marked);
    }
}
