//! JSON output support for DartScope.
//!
//! The [`JsonContract`] registry and [`VersionedJsonEnvelope`] type define the stable,
//! command-facing JSON boundary. The generic [`to_json`] and [`to_json_pretty`] helpers
//! remain available for callers that only need Serde serialization, but their raw output is
//! not a versioned DartScope schema.

use serde::Serialize;

/// A named JSON contract emitted by one DartScope CLI command family.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JsonContract {
    FileAnalysis,
    PubspecAnalysis,
    PubspecConfiguration,
    ProjectAnalysis,
    GraphqlContracts,
    UriGraph,
    FlutterInventory,
    LintAnalysis,
}

impl JsonContract {
    /// Every public CLI JSON contract in deterministic command-family order.
    pub const ALL: [Self; 8] = [
        Self::FileAnalysis,
        Self::PubspecAnalysis,
        Self::PubspecConfiguration,
        Self::ProjectAnalysis,
        Self::GraphqlContracts,
        Self::UriGraph,
        Self::FlutterInventory,
        Self::LintAnalysis,
    ];

    /// Returns the stable schema identifier written into the JSON envelope.
    pub const fn schema(self) -> &'static str {
        match self {
            Self::FileAnalysis => "dartscope.file-analysis",
            Self::PubspecAnalysis => "dartscope.pubspec-analysis",
            Self::PubspecConfiguration => "dartscope.pubspec-configuration",
            Self::ProjectAnalysis => "dartscope.project-analysis",
            Self::GraphqlContracts => "dartscope.graphql-contracts",
            Self::UriGraph => "dartscope.uri-graph",
            Self::FlutterInventory => "dartscope.flutter-inventory",
            Self::LintAnalysis => "dartscope.lint-analysis",
        }
    }

    /// Returns the current major version for this schema.
    pub const fn version(self) -> u16 {
        match self {
            Self::FileAnalysis
            | Self::PubspecAnalysis
            | Self::PubspecConfiguration
            | Self::ProjectAnalysis
            | Self::GraphqlContracts
            | Self::UriGraph
            | Self::FlutterInventory
            | Self::LintAnalysis => 1,
        }
    }

    /// Borrows a payload through this command family's versioned envelope.
    pub const fn envelope<T: ?Sized>(self, data: &T) -> VersionedJsonEnvelope<'_, T> {
        VersionedJsonEnvelope {
            schema: self.schema(),
            version: self.version(),
            data,
        }
    }
}

/// Stable top-level shape for command-facing DartScope JSON.
#[derive(Debug, Serialize)]
pub struct VersionedJsonEnvelope<'a, T: ?Sized> {
    pub schema: &'static str,
    pub version: u16,
    pub data: &'a T,
}

/// Serializes one named versioned contract as compact JSON.
pub fn to_json_contract<T: Serialize + ?Sized>(
    contract: JsonContract,
    value: &T,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&contract.envelope(value))
}

/// Serializes one named versioned contract as pretty JSON.
pub fn to_json_contract_pretty<T: Serialize + ?Sized>(
    contract: JsonContract,
    value: &T,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&contract.envelope(value))
}

/// Serializes an arbitrary Serde value as pretty JSON.
///
/// This helper does not create a stable DartScope schema. Command-facing output should use
/// [`to_json_contract_pretty`] instead.
pub fn to_json_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Serializes an arbitrary Serde value as compact JSON.
///
/// This helper does not create a stable DartScope schema. Command-facing output should use
/// [`to_json_contract`] instead.
pub fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use dartscope_core::DartFileAnalysis;

    #[test]
    fn serializes_raw_analysis_without_claiming_a_contract() {
        let json = to_json_pretty(&DartFileAnalysis::empty("lib/main.dart")).unwrap();

        assert!(json.contains("\"path\": \"lib/main.dart\""));
        assert!(!json.contains("\"schema\""));
    }

    #[test]
    fn wraps_payload_in_named_versioned_envelope() {
        let analysis = DartFileAnalysis::empty("lib/main.dart");
        let json = to_json_contract_pretty(JsonContract::FileAnalysis, &analysis).unwrap();

        assert!(json.starts_with("{\n  \"schema\": \"dartscope.file-analysis\","));
        assert!(json.contains("\n  \"version\": 1,"));
        assert!(json.contains("\n  \"data\": {"));
    }

    #[test]
    fn contract_registry_has_unique_names_and_nonzero_versions() {
        let schemas = JsonContract::ALL
            .into_iter()
            .map(JsonContract::schema)
            .collect::<HashSet<_>>();

        assert_eq!(schemas.len(), JsonContract::ALL.len());
        assert!(
            JsonContract::ALL
                .into_iter()
                .all(|contract| contract.version() > 0)
        );
    }
}
