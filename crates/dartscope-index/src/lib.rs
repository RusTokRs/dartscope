//! Deterministic project indexing and cross-file analysis.

mod graphql;
mod parts;
mod paths;
mod uri_graph;

pub use graphql::{analyze_graphql_contracts, analyze_graphql_contracts_with_options};
pub use parts::analyze_part_links;
pub use uri_graph::{build_uri_graph, build_uri_graph_with_options, DartIndexOptions};

#[cfg(test)]
mod tests;
