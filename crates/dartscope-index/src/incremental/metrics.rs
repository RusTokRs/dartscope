//! Retained-size metrics of the caches of an index.

use dartscope_core::{
    DartGraphqlContractAnalysis, DartIdentifierReferenceResolution, DartUriReference,
};

use super::{DartWorkspaceIndex, DartWorkspaceIndexRetainedMetrics};

impl DartWorkspaceIndex {
    pub fn retained_metrics(&self) -> DartWorkspaceIndexRetainedMetrics {
        let uri_references = self
            .uri_references_by_path
            .values()
            .map(|references| references.len())
            .sum();
        let library_member_paths = self
            .library_paths_by_owner
            .values()
            .map(|paths| paths.len())
            .sum();
        let dependency_references = self
            .library_dependency_fingerprints_by_owner
            .values()
            .map(|fingerprint| fingerprint.references.len())
            .sum();
        let graphql_bindings = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.bindings.len())
            .sum();
        let graphql_unresolved_uses = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.unresolved_uses.len())
            .sum();
        let reference_resolutions = self
            .reference_resolutions_by_path
            .values()
            .map(|resolutions| resolutions.len())
            .sum();
        let retained_path_uri_bytes = self.root.len()
            + self
                .uri_references_by_path
                .iter()
                .map(|(path, references)| {
                    path.len()
                        + references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .library_paths_by_owner
                .iter()
                .map(|(owner, paths)| owner.len() + paths.iter().map(String::len).sum::<usize>())
                .sum::<usize>()
            + self
                .library_dependency_fingerprints_by_owner
                .iter()
                .map(|(owner, fingerprint)| {
                    owner.len()
                        + fingerprint.owner_path.len()
                        + fingerprint
                            .member_paths
                            .iter()
                            .map(String::len)
                            .sum::<usize>()
                        + fingerprint
                            .references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .graphql_contracts_by_library
                .iter()
                .map(|(owner, analysis)| owner.len() + graphql_contract_path_bytes(analysis))
                .sum::<usize>()
            + self
                .reference_resolutions_by_path
                .iter()
                .map(|(path, resolutions)| {
                    path.len()
                        + resolutions
                            .iter()
                            .map(reference_resolution_path_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>();

        DartWorkspaceIndexRetainedMetrics {
            indexed_files: self.files.len(),
            uri_source_entries: self.uri_references_by_path.len(),
            uri_references,
            library_entries: self.library_paths_by_owner.len(),
            library_member_paths,
            dependency_fingerprints: self.library_dependency_fingerprints_by_owner.len(),
            dependency_references,
            graphql_library_entries: self.graphql_contracts_by_library.len(),
            graphql_bindings,
            graphql_unresolved_uses,
            reference_source_entries: self.reference_resolutions_by_path.len(),
            reference_resolutions,
            retained_path_uri_bytes,
        }
    }
}

fn uri_reference_path_uri_bytes(reference: &DartUriReference) -> usize {
    reference.source_path.len()
        + reference.uri.len()
        + reference.condition.as_ref().map_or(0, String::len)
        + reference.target_path.as_ref().map_or(0, String::len)
        + reference.target_uri.as_ref().map_or(0, String::len)
        + reference
            .candidate_paths
            .iter()
            .map(String::len)
            .sum::<usize>()
}

fn graphql_contract_path_bytes(analysis: &DartGraphqlContractAnalysis) -> usize {
    analysis
        .bindings
        .iter()
        .map(|binding| binding.operation_path.len() + binding.use_path.len())
        .sum::<usize>()
        + analysis
            .unresolved_uses
            .iter()
            .map(|operation_use| {
                operation_use.use_path.len()
                    + operation_use
                        .candidate_paths
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
            })
            .sum::<usize>()
}

fn reference_resolution_path_bytes(resolution: &DartIdentifierReferenceResolution) -> usize {
    resolution.reference.source_path.len()
        + resolution
            .candidates
            .iter()
            .map(|candidate| candidate.declaration_path.len())
            .sum::<usize>()
}
