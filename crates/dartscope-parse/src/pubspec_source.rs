use dartscope_core::PubspecDependency;
pub use dartscope_core::pubspec::{
    parse_normalized_dependency_source, PubspecDependencySource, PubspecDependencySourceField,
};

/// Compatibility extension for callers importing the pre-migration parse-crate trait.
pub trait PubspecDependencySourceExt {
    fn structured_source(&self) -> Option<PubspecDependencySource>;
}

impl PubspecDependencySourceExt for PubspecDependency {
    fn structured_source(&self) -> Option<PubspecDependencySource> {
        PubspecDependency::structured_source(self)
    }
}

#[cfg(test)]
mod tests {
    use dartscope_core::{PubspecDependencySection, PubspecInput, SourceSpan};

    use super::*;

    #[test]
    fn exposes_typed_sources_from_legacy_normalized_values() {
        let version_or_source =
            "git:ref=stable;url=https://example.com/repo.git;version=^1.0.0".to_string();
        let dependency = PubspecDependency::new(
            "remote_package",
            PubspecDependencySection::Dependencies,
            Some(version_or_source.clone()),
            Some(parse_normalized_dependency_source(&version_or_source)),
            SourceSpan::line(1, 0, "remote_package"),
        );

        assert_eq!(
            PubspecDependencySourceExt::structured_source(&dependency),
            Some(PubspecDependencySource::Git {
                url: Some("https://example.com/repo.git".to_string()),
                reference: Some("stable".to_string()),
                path: None,
                version: Some("^1.0.0".to_string()),
                additional_fields: Vec::new(),
            })
        );
    }

    #[test]
    fn exposes_every_parsed_dependency_source_variant() {
        let source = r#"name: demo
dependencies:
  plain: ^1.2.0
  flutter:
    sdk: flutter
  local_package:
    path: ../local_package
  remote_package:
    git:
      url: https://example.com/repo.git
      ref: stable
    version: ^1.0.0
  hosted_package:
    hosted:
      name: hosted_package
      url: https://pub.example.com
    version: ^2.0.0
  workspace_package:
    workspace: true
"#;
        let analysis = crate::parse_pubspec(PubspecInput::new("pubspec.yaml", source));
        let source_for = |name: &str| {
            analysis
                .dependencies
                .iter()
                .find(|dependency| dependency.name == name)
                .and_then(PubspecDependencySourceExt::structured_source)
        };

        assert_eq!(
            source_for("plain"),
            Some(PubspecDependencySource::Version {
                constraint: "^1.2.0".to_string(),
            })
        );
        assert_eq!(
            source_for("flutter"),
            Some(PubspecDependencySource::Sdk {
                sdk: "flutter".to_string(),
            })
        );
        assert_eq!(
            source_for("local_package"),
            Some(PubspecDependencySource::Path {
                path: "../local_package".to_string(),
            })
        );
        assert_eq!(
            source_for("remote_package"),
            Some(PubspecDependencySource::Git {
                url: Some("https://example.com/repo.git".to_string()),
                reference: Some("stable".to_string()),
                path: None,
                version: Some("^1.0.0".to_string()),
                additional_fields: Vec::new(),
            })
        );
        assert_eq!(
            source_for("hosted_package"),
            Some(PubspecDependencySource::Hosted {
                name: Some("hosted_package".to_string()),
                url: Some("https://pub.example.com".to_string()),
                version: Some("^2.0.0".to_string()),
                additional_fields: Vec::new(),
            })
        );
        assert_eq!(
            source_for("workspace_package"),
            Some(PubspecDependencySource::Workspace)
        );
    }

    #[test]
    fn preserves_unknown_source_fields() {
        assert_eq!(
            parse_normalized_dependency_source("git:custom=value;url=https://example.com/repo.git"),
            PubspecDependencySource::Git {
                url: Some("https://example.com/repo.git".to_string()),
                reference: None,
                path: None,
                version: None,
                additional_fields: vec![PubspecDependencySourceField {
                    key: "custom".to_string(),
                    value: "value".to_string(),
                }],
            }
        );
        assert_eq!(
            parse_normalized_dependency_source("custom=value"),
            PubspecDependencySource::Other {
                value: "custom=value".to_string(),
            }
        );
    }

    #[test]
    fn keeps_direct_source_urls_with_query_values_intact() {
        assert_eq!(
            parse_normalized_dependency_source(
                "git:https://example.com/repo.git?ref=stable&depth=1"
            ),
            PubspecDependencySource::Git {
                url: Some("https://example.com/repo.git?ref=stable&depth=1".to_string()),
                reference: None,
                path: None,
                version: None,
                additional_fields: Vec::new(),
            }
        );
        assert_eq!(
            parse_normalized_dependency_source("hosted:https://pub.example.com?token=demo"),
            PubspecDependencySource::Hosted {
                name: None,
                url: Some("https://pub.example.com?token=demo".to_string()),
                version: None,
                additional_fields: Vec::new(),
            }
        );
    }

    #[test]
    fn serializes_with_a_stable_kind_discriminator() {
        let value = serde_json::to_value(PubspecDependencySource::Path {
            path: "../local_package".to_string(),
        })
        .expect("serialize source");

        assert_eq!(value["kind"], "path");
        assert_eq!(value["path"], "../local_package");
    }
}
