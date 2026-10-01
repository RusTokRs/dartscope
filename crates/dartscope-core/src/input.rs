//! The source text a caller hands to an analyzer: files, pubspecs, package configurations, projects.

use serde::{Deserialize, Serialize};

use crate::path::normalize_path;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartFileInput {
    pub path: String,
    pub source: String,
}

impl DartFileInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubspecInput {
    pub path: String,
    pub source: String,
}

impl PubspecInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageConfigInput {
    pub path: String,
    pub source: String,
}

impl PackageConfigInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartProjectInput {
    pub root: String,
    pub files: Vec<DartFileInput>,
    pub pubspecs: Vec<PubspecInput>,
    #[serde(default)]
    pub package_configs: Vec<PackageConfigInput>,
}

impl DartProjectInput {
    pub fn new(
        root: impl Into<String>,
        files: Vec<DartFileInput>,
        pubspecs: Vec<PubspecInput>,
    ) -> Self {
        Self {
            root: normalize_path(root.into()),
            files,
            pubspecs,
            package_configs: Vec::new(),
        }
    }

    pub fn with_package_configs(mut self, package_configs: Vec<PackageConfigInput>) -> Self {
        self.package_configs = package_configs;
        self
    }
}
