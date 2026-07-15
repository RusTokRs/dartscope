use dartscope_core::PubspecDependency;
pub use dartscope_core::pubspec::{
    PubspecDependencySource, PubspecDependencySourceField,
};

/// Converts the compatibility value on a parsed dependency into a typed source.
pub trait PubspecDependencySourceExt {
    fn structured_source(&self) -> Option<PubspecDependencySource>;
}

impl PubspecDependencySourceExt for PubspecDependency {
    fn structured_source(&self) -> Option<PubspecDependencySource> {
        self.version_or_source
            .as_deref()
            .map(parse_normalized_dependency_source)
    }
}

/// Parses the deterministic source representation emitted by the current pubspec parser.
pub fn parse_normalized_dependency_source(value: &str) -> PubspecDependencySource {
    if value == "workspace" {
        return PubspecDependencySource::Workspace;
    }
    if let Some(sdk) = value.strip_prefix("sdk:") {
        return PubspecDependencySource::Sdk {
            sdk: sdk.to_string(),
        };
    }
    if let Some(path) = value.strip_prefix("path:") {
        return PubspecDependencySource::Path {
            path: path.to_string(),
        };
    }
    if let Some(source) = value.strip_prefix("git:") {
        return parse_git_source(source);
    }
    if let Some(source) = value.strip_prefix("hosted:") {
        return parse_hosted_source(source);
    }
    if looks_like_field_list(value) {
        return PubspecDependencySource::Other {
            value: value.to_string(),
        };
    }

    PubspecDependencySource::Version {
        constraint: value.to_string(),
    }
}

fn parse_git_source(source: &str) -> PubspecDependencySource {
    if !looks_like_field_list(source) {
        return PubspecDependencySource::Git {
            url: non_empty(source),
            reference: None,
            path: None,
            version: None,
            additional_fields: Vec::new(),
        };
    }

    let mut url = None;
    let mut reference = None;
    let mut path = None;
    let mut version = None;
    let mut additional_fields = Vec::new();
    for field in parse_fields(source) {
        match field.key.as_str() {
            "url" => url = Some(field.value),
            "ref" => reference = Some(field.value),
            "path" => path = Some(field.value),
            "version" => version = Some(field.value),
            _ => additional_fields.push(field),
        }
    }

    PubspecDependencySource::Git {
        url,
        reference,
        path,
        version,
        additional_fields,
    }
}

fn parse_hosted_source(source: &str) -> PubspecDependencySource {
    if !looks_like_field_list(source) {
        return PubspecDependencySource::Hosted {
            name: None,
            url: non_empty(source),
            version: None,
            additional_fields: Vec::new(),
        };
    }

    let mut name = None;
    let mut url = None;
    let mut version = None;
    let mut additional_fields = Vec::new();
    for field in parse_fields(source) {
        match field.key.as_str() {
            "name" => name = Some(field.value),
            "url" => url = Some(field.value),
            "version" => version = Some(field.value),
            _ => additional_fields.push(field),
        }
    }

    PubspecDependencySource::Hosted {
        name,
        url,
        version,
        additional_fields,
    }
}

fn looks_like_field_list(source: &str) -> bool {
    !source.is_empty()
        && source.split(';').all(|field| {
            field.split_once('=').is_some_and(|(key, _)| {
                !key.is_empty()
                    && key
                        .chars()
                        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
            })
        })
}

fn parse_fields(source: &str) -> Vec<PubspecDependencySourceField> {
    source
        .split(';')
        .filter_map(|field| {
            let (key, value) = field.split_once('=')?;
            Some(PubspecDependencySourceField {
                key: key.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use dartscope_core::{PubspecDependencySection, PubspecInput, SourceSpan};

    use super::*;

    #[test]
    fn exposes_typed_sources_from_legacy_normalized_values() {
        let dependency = PubspecDependency {
            name: "remote_package".to_string(),
            section: PubspecDependencySection::Dependencies,
            version_or_source: Some(
                "git:ref=stable;url=https://example.com/repo.git;version=^1.0.0".to_string(),
            ),
            span: SourceSpan::line(1, 0, "remote_package"),
        };

        assert_eq!(
            dependency.structured_source(),
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
