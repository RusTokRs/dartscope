//! Library directives: imports, exports, parts, `part of`, combinators, conditional configurations.

use serde::{Deserialize, Serialize};

use crate::span::SourceSpan;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartImport {
    pub uri: String,
    pub configurations: Vec<DartUriConfiguration>,
    pub is_deferred: bool,
    pub prefix: Option<String>,
    pub combinators: Vec<DartNamespaceCombinator>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartExport {
    pub uri: String,
    pub configurations: Vec<DartUriConfiguration>,
    pub combinators: Vec<DartNamespaceCombinator>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartUriConfiguration {
    pub condition: String,
    pub uri: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct DartCompilationEnvironment {
    pub entries: Vec<DartCompilationEnvironmentEntry>,
}

impl DartCompilationEnvironment {
    pub fn new(entries: Vec<DartCompilationEnvironmentEntry>) -> Self {
        Self { entries }
    }

    pub fn from_pairs(
        pairs: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            entries: pairs
                .into_iter()
                .map(|(key, value)| DartCompilationEnvironmentEntry {
                    key: key.into(),
                    value: value.into(),
                })
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartCompilationEnvironmentEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartNamespaceCombinator {
    pub kind: DartNamespaceCombinatorKind,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartNamespaceCombinatorKind {
    Show,
    Hide,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPart {
    pub uri: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartPartOf {
    pub library: String,
    pub kind: DartPartOfKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLibraryDirective {
    pub name: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartPartOfKind {
    Uri,
    LibraryName,
}
