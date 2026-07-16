use serde::{Deserialize, Serialize};

use crate::{DartDiagnostic, PubspecDependency, PubspecDependencySection, SourceSpan};

/// A normalized pubspec dependency source.
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        additional_fields: Vec<PubspecDependencySourceField>,
    },
    Hosted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        additional_fields: Vec<PubspecDependencySourceField>,
    },
    Workspace,
    Other {
        value: String,
    },
}

/// A dependency source field outside the common git or hosted shape.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecDependencySourceField {
    pub key: String,
    pub value: String,
}

/// Configuration embedded in the primary pubspec analysis model.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecConfiguration {
    pub environment: Vec<PubspecEnvironmentConstraint>,
    pub flutter: PubspecFlutterConfiguration,
}

/// Typed pubspec configuration outside dependency discovery.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecConfigurationAnalysis {
    pub path: String,
    pub environment: Vec<PubspecEnvironmentConstraint>,
    pub flutter: PubspecFlutterConfiguration,
    pub diagnostics: Vec<DartDiagnostic>,
}

impl PubspecConfigurationAnalysis {
    /// Removes analysis-only path and diagnostics for embedding in `PubspecAnalysis`.
    pub fn into_configuration(self) -> PubspecConfiguration {
        PubspecConfiguration {
            environment: self.environment,
            flutter: self.flutter,
        }
    }
}

/// One top-level pubspec environment constraint.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecEnvironmentConstraint {
    pub name: String,
    pub constraint: String,
    pub span: SourceSpan,
}

/// Versioned DartScope policy for Flutter asset flavor and platform selectors.
///
/// Version 1 keeps flavor names as non-empty opaque application values and validates
/// platforms against Flutter's documented six-platform list. The version belongs to the
/// DartScope output contract rather than a specific Flutter SDK release.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
pub enum PubspecFlutterAssetSelectorPolicy {
    #[default]
    #[serde(rename = "v1")]
    V1,
}

impl PubspecFlutterAssetSelectorPolicy {
    pub const CURRENT: Self = Self::V1;

    /// Returns the stable numeric policy version.
    pub const fn version(self) -> u16 {
        match self {
            Self::V1 => 1,
        }
    }

    /// Returns the platform names accepted by this policy.
    pub const fn supported_platforms(self) -> &'static [&'static str] {
        match self {
            Self::V1 => &["android", "ios", "web", "linux", "macos", "windows"],
        }
    }

    /// Returns whether a flavor name is valid under this policy.
    pub fn accepts_flavor(self, flavor: &str) -> bool {
        match self {
            Self::V1 => !flavor.is_empty(),
        }
    }

    /// Returns whether a platform name is valid under this policy.
    pub fn accepts_platform(self, platform: &str) -> bool {
        self.supported_platforms().contains(&platform)
    }
}

/// Normalized configuration owned by the top-level Flutter mapping.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecFlutterConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_material_design: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_localizations: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_flavor: Option<String>,
    #[serde(default)]
    pub asset_selector_policy: PubspecFlutterAssetSelectorPolicy,
    pub assets: Vec<PubspecFlutterAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_configurations: Vec<PubspecFlutterAssetConfiguration>,
    pub fonts: Vec<PubspecFlutterFontFamily>,
}

/// A scalar or path-mapping Flutter asset entry retained for pre-1.0 compatibility.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterAsset {
    pub path: String,
    pub span: SourceSpan,
}

/// A complete Flutter asset declaration, including optional selectors and transforms.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterAssetConfiguration {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flavors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transformers: Vec<PubspecFlutterAssetTransformer>,
    pub span: SourceSpan,
}

/// One ordered build-time transformer attached to a Flutter asset.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterAssetTransformer {
    pub package: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    pub span: SourceSpan,
}

/// A Flutter font family from a pubspec.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterFontFamily {
    pub family: String,
    pub fonts: Vec<PubspecFlutterFont>,
    pub span: SourceSpan,
}

/// A concrete Flutter font asset.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterFont {
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    pub span: SourceSpan,
}

impl PubspecDependency {
    /// Constructs a dependency with typed and legacy source representations.
    pub fn new(
        name: impl Into<String>,
        section: PubspecDependencySection,
        version_or_source: Option<String>,
        source: Option<PubspecDependencySource>,
        span: SourceSpan,
    ) -> Self {
        debug_assert_eq!(
            source,
            version_or_source
                .as_deref()
                .map(parse_normalized_dependency_source)
        );
        let mut dependency = Self {
            name: name.into(),
            section,
            source,
            version_or_source,
            ..Self::default()
        };
        dependency.span = span;
        dependency
    }

    /// Returns the stored typed source, with a fallback for legacy deserialized values.
    pub fn structured_source(&self) -> Option<PubspecDependencySource> {
        self.source.clone().or_else(|| {
            self.version_or_source
                .as_deref()
                .map(parse_normalized_dependency_source)
        })
    }
}

/// Parses the deterministic dependency-source representation emitted by DartScope.
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
    use super::*;

    #[test]
    fn parses_common_dependency_source_variants() {
        assert_eq!(
            parse_normalized_dependency_source("sdk:flutter"),
            PubspecDependencySource::Sdk {
                sdk: "flutter".to_string(),
            }
        );
        assert_eq!(
            parse_normalized_dependency_source("path:../local"),
            PubspecDependencySource::Path {
                path: "../local".to_string(),
            }
        );
        assert_eq!(
            parse_normalized_dependency_source("workspace"),
            PubspecDependencySource::Workspace
        );
    }

    #[test]
    fn preserves_direct_urls_and_additional_fields() {
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
    fn extracts_embeddable_configuration() {
        let analysis = PubspecConfigurationAnalysis {
            path: "pubspec.yaml".to_string(),
            environment: Vec::new(),
            flutter: PubspecFlutterConfiguration::default(),
            diagnostics: Vec::new(),
        };

        assert_eq!(
            analysis.into_configuration(),
            PubspecConfiguration::default()
        );
    }

    #[test]
    fn defaults_extended_asset_configuration_for_legacy_json() {
        let flutter: PubspecFlutterConfiguration =
            serde_json::from_value(serde_json::json!({"assets": [], "fonts": []}))
                .expect("deserialize legacy Flutter configuration");

        assert!(flutter.asset_configurations.is_empty());
    }
}
