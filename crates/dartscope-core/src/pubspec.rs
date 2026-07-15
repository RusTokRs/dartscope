use serde::{Deserialize, Serialize};

use crate::{
    DartDiagnostic, PubspecDependency, PubspecDependencySection, SourceSpan,
};

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

/// Typed pubspec configuration outside dependency discovery.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecConfigurationAnalysis {
    pub path: String,
    pub environment: Vec<PubspecEnvironmentConstraint>,
    pub flutter: PubspecFlutterConfiguration,
    pub diagnostics: Vec<DartDiagnostic>,
}

/// One top-level pubspec environment constraint.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecEnvironmentConstraint {
    pub name: String,
    pub constraint: String,
    pub span: SourceSpan,
}

/// Normalized configuration owned by the top-level Flutter mapping.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecFlutterConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_material_design: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_localizations: Option<bool>,
    pub assets: Vec<PubspecFlutterAsset>,
    pub fonts: Vec<PubspecFlutterFontFamily>,
}

/// A scalar or path-mapping Flutter asset entry.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecFlutterAsset {
    pub path: String,
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
    /// Constructs a dependency while the legacy and typed source fields are migrated.
    pub fn new(
        name: impl Into<String>,
        section: PubspecDependencySection,
        version_or_source: Option<String>,
        _source: Option<PubspecDependencySource>,
        span: SourceSpan,
    ) -> Self {
        Self {
            name: name.into(),
            section,
            version_or_source,
            span,
        }
    }
}
