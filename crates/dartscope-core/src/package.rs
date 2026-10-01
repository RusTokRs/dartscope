//! Package-configuration and pubspec analysis results.

use serde::{Deserialize, Serialize};

use crate::diagnostic::DartDiagnostic;
use crate::span::SourceSpan;
use crate::pubspec::{PubspecConfiguration, PubspecDependencySource};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageConfigAnalysis {
    pub path: String,
    pub config_version: Option<u64>,
    pub packages: Vec<DartPackageConfigEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator_version: Option<String>,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPackageConfigEntry {
    pub name: String,
    pub root_uri: String,
    pub package_uri: Option<String>,
    pub language_version: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartResolvedPackageUri {
    pub package_name: String,
    pub resolved_uri: String,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecAnalysis {
    pub path: String,
    pub package_name: Option<String>,
    pub dependencies: Vec<PubspecDependency>,
    #[serde(default)]
    pub configuration: PubspecConfiguration,
    pub diagnostics: Vec<DartDiagnostic>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PubspecDependency {
    pub name: String,
    pub section: PubspecDependencySection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PubspecDependencySource>,
    pub version_or_source: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PubspecDependencySection {
    #[default]
    Dependencies,
    DevDependencies,
    DependencyOverrides,
}
