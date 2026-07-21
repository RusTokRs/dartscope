//! Deterministic project indexing and cross-file analysis.

mod graphql;
mod incremental;
mod lexical_bindings;
mod namespace;
mod navigation;
mod parts;
mod paths;
mod references;
mod uri_graph;

pub use graphql::{analyze_graphql_contracts, analyze_graphql_contracts_with_options};
pub use incremental::{
    DartLibraryDependencyFingerprint, DartWorkspaceIndex, DartWorkspaceIndexCounters,
    DartWorkspaceIndexRetainedMetrics, DartWorkspaceSnapshot, DartWorkspaceSubsystems,
    DartWorkspaceUpdate,
};
pub use lexical_bindings::{
    resolve_project_lexical_binding, resolve_project_variable_read_references,
    resolve_project_variable_write_references,
};
pub use namespace::{resolve_symbol, resolve_symbol_with_options};
pub use navigation::{
    DartDefinitionBatchAnalysis, DartDefinitionQuery, DartDefinitionResolution,
    DartDefinitionResolutionStatus, DartDefinitionTarget, DartReferenceBatchAnalysis,
    DartReferenceSearchResult, DartWorkspaceResolutionContext, find_definitions,
    find_definitions_with_options, find_references, find_references_with_options,
};
pub use parts::analyze_part_links;
pub use references::{
    resolve_identifier_references, resolve_identifier_references_with_options,
    resolve_project_identifier_references, resolve_project_identifier_references_with_options,
};
pub use uri_graph::{DartIndexOptions, build_uri_graph, build_uri_graph_with_options};

#[cfg(test)]
mod tests;
