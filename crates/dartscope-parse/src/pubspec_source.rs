use dartscope_core::PubspecDependency;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PubspecDependencySource {
    Version {
        constraint: String,
    },
    Sdk {
        sdk: String,
    },
    Path {
        path: String,
    },
    Git {
        url: Option<String>,
        reference: Option<String>,
        path: Option<String>,
        version: Option<String>,
        additional_fields: Vec<PubspecDependencySourceField>,
    },
    Hosted {
        name: Option<String>,
        url: Option<String>,
        version: Option<String>,
        additional_fields: Vec<PubspecDependencySourceField>,
    },
    Workspace,
    Other {
        value: String,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecDependencySourceField {
    pub key: String,
    pub value: String,
}

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
    if value.contains('=') && value.contains(';') {
        return PubspecDependencySource::Other {
            value: value.to_string(),
        };
    }

    PubspecDependencySource::Version {
        constraint: value.to_string(),
    }
}

fn parse_git_source(source: &str) -> PubspecDependencySource {
    if !source.contains('=') {
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
    if !source.contains('=') {
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
    use dartscope_core::{PubspecDependencySection, SourceSpan};

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
