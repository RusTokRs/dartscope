use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::Arc;

use dartscope_core::{
    DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBinding,
    DartLexicalBindingQuery, DartLexicalBindingResolutionStatus, DartNamespaceCombinatorKind,
    DartProjectReferenceAnalysis, DartSymbolCandidate, DartSymbolQuery, DartSymbolResolutionStatus,
    DartUriGraph, DartUriReferenceKind, DartUriResolution, normalize_path,
};

use crate::lexical_bindings::resolve_lexical_binding;
use crate::namespace::{NamespaceResolver, resolve_symbol_with_resolver};
use crate::parts::analyze_part_links_with_graph;
use crate::uri_graph::{DartIndexOptions, build_uri_graph_with_options};

/// One editor-style definition lookup at a normalized source byte position.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartDefinitionQuery {
    pub source_path: String,
    pub byte_offset: usize,
}

impl DartDefinitionQuery {
    pub fn new(source_path: impl Into<String>, byte_offset: usize) -> Self {
        Self {
            source_path: normalize_path(source_path.into()),
            byte_offset,
        }
    }
}

/// Unified namespace or lexical target retained as definition evidence.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DartDefinitionTarget {
    Namespace(DartSymbolCandidate),
    Lexical(DartLexicalBinding),
}

/// Explicit result state for one definition query.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DartDefinitionResolutionStatus {
    Resolved,
    ReferenceMissing,
    Missing,
    Ambiguous,
    NotVisible,
    ConditionalEnvironmentRequired,
    ExternalUnindexed,
    SourceFileMissing,
}

/// Definition result for all parser-produced reference facts covering one position.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartDefinitionResolution {
    pub query: DartDefinitionQuery,
    pub references: Vec<DartIdentifierReference>,
    pub status: DartDefinitionResolutionStatus,
    pub targets: Vec<DartDefinitionTarget>,
    pub external_uris: Vec<String>,
}

/// Deterministically ordered batch of definition results.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DartDefinitionBatchAnalysis {
    pub resolutions: Vec<DartDefinitionResolution>,
}

/// Reverse-reference result for one selected definition target.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartReferenceSearchResult {
    pub target: DartDefinitionTarget,
    pub references: Vec<DartIdentifierReference>,
}

/// Deterministically ordered reverse-reference batch.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DartReferenceBatchAnalysis {
    pub results: Vec<DartReferenceSearchResult>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ResolvedReference {
    reference: DartIdentifierReference,
    status: DartDefinitionResolutionStatus,
    targets: Vec<DartDefinitionTarget>,
    external_uris: Vec<String>,
}

/// Reusable resolution context over one normalized parser-produced workspace snapshot.
///
/// Construction builds the URI/namespace context once, resolves every namespace and lexical fact,
/// and retains no raw Dart source. Repeated definition/reference batches reuse those results.
#[derive(Debug, Clone)]
pub struct DartWorkspaceResolutionContext {
    source_paths: BTreeSet<String>,
    resolutions: Vec<ResolvedReference>,
}

impl DartWorkspaceResolutionContext {
    pub fn new(analysis: &DartProjectReferenceAnalysis) -> Self {
        Self::with_options(analysis, &DartIndexOptions::default())
    }

    pub fn with_options(
        analysis: &DartProjectReferenceAnalysis,
        options: &DartIndexOptions,
    ) -> Self {
        let uri_graph = Arc::new(build_uri_graph_with_options(&analysis.project, options));
        let part_links = analyze_part_links_with_graph(&analysis.project, &uri_graph);
        let namespace = NamespaceResolver::from_analyses(
            &analysis.project,
            options,
            Arc::clone(&uri_graph),
            &part_links,
        );
        let mut references = analysis.references.clone();
        sort_references(&mut references);
        let mut resolutions = references
            .into_iter()
            .map(|reference| resolve_reference(analysis, &namespace, &uri_graph, reference))
            .collect::<Vec<_>>();
        resolutions.sort_by(|left, right| compare_references(&left.reference, &right.reference));
        Self {
            source_paths: analysis
                .project
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect(),
            resolutions,
        }
    }

    pub fn find_definitions(&self, queries: &[DartDefinitionQuery]) -> DartDefinitionBatchAnalysis {
        let mut queries = queries.to_vec();
        queries.sort_by(|left, right| {
            (&left.source_path, left.byte_offset).cmp(&(&right.source_path, right.byte_offset))
        });
        queries.dedup();
        DartDefinitionBatchAnalysis {
            resolutions: queries
                .into_iter()
                .map(|query| self.definition_for_query(query))
                .collect(),
        }
    }

    pub fn find_references(&self, targets: &[DartDefinitionTarget]) -> DartReferenceBatchAnalysis {
        let mut targets = targets.to_vec();
        targets.sort_by(compare_targets);
        targets.dedup_by(|left, right| same_target(left, right));
        let results = targets
            .into_iter()
            .map(|target| {
                let mut references = self
                    .resolutions
                    .iter()
                    .filter(|resolution| {
                        resolution.status == DartDefinitionResolutionStatus::Resolved
                            && resolution.targets.len() == 1
                            && same_target(&resolution.targets[0], &target)
                    })
                    .map(|resolution| resolution.reference.clone())
                    .collect::<Vec<_>>();
                sort_references(&mut references);
                references.dedup();
                DartReferenceSearchResult { target, references }
            })
            .collect();
        DartReferenceBatchAnalysis { results }
    }

    fn definition_for_query(&self, query: DartDefinitionQuery) -> DartDefinitionResolution {
        if !self.source_paths.contains(&query.source_path) {
            return DartDefinitionResolution {
                query,
                references: Vec::new(),
                status: DartDefinitionResolutionStatus::SourceFileMissing,
                targets: Vec::new(),
                external_uris: Vec::new(),
            };
        }
        let matches = self
            .resolutions
            .iter()
            .filter(|resolution| {
                resolution.reference.source_path == query.source_path
                    && resolution.reference.span.byte_start <= query.byte_offset
                    && query.byte_offset < resolution.reference.span.byte_end
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return DartDefinitionResolution {
                query,
                references: Vec::new(),
                status: DartDefinitionResolutionStatus::ReferenceMissing,
                targets: Vec::new(),
                external_uris: Vec::new(),
            };
        }

        let mut references = matches
            .iter()
            .map(|resolution| resolution.reference.clone())
            .collect::<Vec<_>>();
        sort_references(&mut references);
        references.dedup();
        let mut targets = matches
            .iter()
            .flat_map(|resolution| resolution.targets.iter().cloned())
            .collect::<Vec<_>>();
        targets.sort_by(compare_targets);
        targets.dedup_by(|left, right| same_target(left, right));
        let mut external_uris = matches
            .iter()
            .flat_map(|resolution| resolution.external_uris.iter().cloned())
            .collect::<Vec<_>>();
        external_uris.sort();
        external_uris.dedup();
        let statuses = matches
            .iter()
            .map(|resolution| resolution.status)
            .collect::<Vec<_>>();
        let status = combine_statuses(&statuses, targets.len());
        DartDefinitionResolution {
            query,
            references,
            status,
            targets,
            external_uris,
        }
    }
}

/// Builds one default context and resolves a definition-query batch.
pub fn find_definitions(
    analysis: &DartProjectReferenceAnalysis,
    queries: &[DartDefinitionQuery],
) -> DartDefinitionBatchAnalysis {
    DartWorkspaceResolutionContext::new(analysis).find_definitions(queries)
}

/// Builds one context with explicit index options and resolves a definition-query batch.
pub fn find_definitions_with_options(
    analysis: &DartProjectReferenceAnalysis,
    queries: &[DartDefinitionQuery],
    options: &DartIndexOptions,
) -> DartDefinitionBatchAnalysis {
    DartWorkspaceResolutionContext::with_options(analysis, options).find_definitions(queries)
}

/// Builds one default context and finds references for a target batch.
pub fn find_references(
    analysis: &DartProjectReferenceAnalysis,
    targets: &[DartDefinitionTarget],
) -> DartReferenceBatchAnalysis {
    DartWorkspaceResolutionContext::new(analysis).find_references(targets)
}

/// Builds one context with explicit options and finds references for a target batch.
pub fn find_references_with_options(
    analysis: &DartProjectReferenceAnalysis,
    targets: &[DartDefinitionTarget],
    options: &DartIndexOptions,
) -> DartReferenceBatchAnalysis {
    DartWorkspaceResolutionContext::with_options(analysis, options).find_references(targets)
}

fn resolve_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    if matches!(
        reference.kind,
        DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
    ) {
        return resolve_lexical_reference(analysis, reference);
    }

    let query = DartSymbolQuery {
        source_path: reference.source_path.clone(),
        name: reference.name.clone(),
        prefix: reference.prefix.clone(),
    };
    let resolution = resolve_symbol_with_resolver(&analysis.project, query, namespace);
    let external_uris = external_namespace_uris(analysis, uri_graph, &reference);
    let status = match resolution.status {
        DartSymbolResolutionStatus::Resolved => DartDefinitionResolutionStatus::Resolved,
        DartSymbolResolutionStatus::Missing if !external_uris.is_empty() => {
            DartDefinitionResolutionStatus::ExternalUnindexed
        }
        DartSymbolResolutionStatus::Missing => DartDefinitionResolutionStatus::Missing,
        DartSymbolResolutionStatus::Ambiguous => DartDefinitionResolutionStatus::Ambiguous,
        DartSymbolResolutionStatus::NotVisible => DartDefinitionResolutionStatus::NotVisible,
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => {
            DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
        }
        DartSymbolResolutionStatus::SourceFileMissing => {
            DartDefinitionResolutionStatus::SourceFileMissing
        }
    };
    let mut targets = resolution
        .candidates
        .into_iter()
        .map(DartDefinitionTarget::Namespace)
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris,
    }
}

fn resolve_lexical_reference(
    analysis: &DartProjectReferenceAnalysis,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let mut query = DartLexicalBindingQuery::new(
        reference.source_path.clone(),
        reference.name.clone(),
        reference.span.byte_start,
    );
    if let Some(owner) = reference.enclosing_symbol_id.as_ref() {
        query = query.with_enclosing_symbol_id(owner.clone());
    }
    let resolution = resolve_lexical_binding(analysis, query);
    let status = match resolution.status {
        DartLexicalBindingResolutionStatus::Resolved => DartDefinitionResolutionStatus::Resolved,
        DartLexicalBindingResolutionStatus::Missing => DartDefinitionResolutionStatus::Missing,
        DartLexicalBindingResolutionStatus::Ambiguous => DartDefinitionResolutionStatus::Ambiguous,
        DartLexicalBindingResolutionStatus::SourceFileMissing => {
            DartDefinitionResolutionStatus::SourceFileMissing
        }
    };
    let mut targets = resolution
        .candidates
        .into_iter()
        .map(DartDefinitionTarget::Lexical)
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris: Vec::new(),
    }
}

fn external_namespace_uris(
    analysis: &DartProjectReferenceAnalysis,
    uri_graph: &DartUriGraph,
    reference: &DartIdentifierReference,
) -> Vec<String> {
    if reference.name.starts_with('_') {
        return Vec::new();
    }
    let Some(file) = analysis
        .project
        .files
        .iter()
        .find(|file| file.path == reference.source_path)
    else {
        return Vec::new();
    };
    let mut uris = file
        .imports
        .iter()
        .filter(|import| import_matches_reference(import, reference))
        .filter(|import| namespace_allows_name(&import.combinators, &reference.name))
        .filter(|import| {
            uri_graph.references.iter().any(|uri_reference| {
                uri_reference.kind == DartUriReferenceKind::Import
                    && uri_reference.source_path == file.path
                    && uri_reference.source_span.byte_start == import.span.byte_start
                    && matches!(
                        uri_reference.resolution,
                        DartUriResolution::ResolvedExternal
                            | DartUriResolution::External
                            | DartUriResolution::UnindexedPackage
                    )
            })
        })
        .map(|import| import.uri.clone())
        .collect::<Vec<_>>();
    uris.sort();
    uris.dedup();
    uris
}

fn import_matches_reference(
    import: &dartscope_core::DartImport,
    reference: &DartIdentifierReference,
) -> bool {
    match reference.prefix.as_deref() {
        Some(prefix) => import.prefix.as_deref() == Some(prefix),
        None => import.prefix.is_none() && !import.is_deferred,
    }
}

fn namespace_allows_name(
    combinators: &[dartscope_core::DartNamespaceCombinator],
    name: &str,
) -> bool {
    combinators.iter().all(|combinator| match combinator.kind {
        DartNamespaceCombinatorKind::Show => combinator.names.iter().any(|shown| shown == name),
        DartNamespaceCombinatorKind::Hide => combinator.names.iter().all(|hidden| hidden != name),
    })
}

fn combine_statuses(
    statuses: &[DartDefinitionResolutionStatus],
    target_count: usize,
) -> DartDefinitionResolutionStatus {
    if statuses
        .iter()
        .all(|status| *status == DartDefinitionResolutionStatus::Resolved)
    {
        return if target_count == 1 {
            DartDefinitionResolutionStatus::Resolved
        } else if target_count > 1 {
            DartDefinitionResolutionStatus::Ambiguous
        } else {
            DartDefinitionResolutionStatus::Missing
        };
    }
    if statuses
        .iter()
        .any(|status| *status == DartDefinitionResolutionStatus::Resolved)
    {
        return DartDefinitionResolutionStatus::Ambiguous;
    }
    for status in [
        DartDefinitionResolutionStatus::SourceFileMissing,
        DartDefinitionResolutionStatus::Ambiguous,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired,
        DartDefinitionResolutionStatus::NotVisible,
        DartDefinitionResolutionStatus::ExternalUnindexed,
        DartDefinitionResolutionStatus::Missing,
        DartDefinitionResolutionStatus::ReferenceMissing,
    ] {
        if statuses.iter().any(|candidate| *candidate == status) {
            return status;
        }
    }
    DartDefinitionResolutionStatus::Missing
}

fn compare_targets(left: &DartDefinitionTarget, right: &DartDefinitionTarget) -> Ordering {
    target_sort_key(left).cmp(&target_sort_key(right))
}

fn target_sort_key(target: &DartDefinitionTarget) -> (u8, &str, usize, usize, &str, Option<&str>) {
    match target {
        DartDefinitionTarget::Namespace(candidate) => (
            0,
            candidate.declaration_path.as_str(),
            candidate.declaration_span.byte_start,
            candidate.declaration_span.byte_end,
            candidate.name.as_str(),
            candidate.symbol_id.as_deref(),
        ),
        DartDefinitionTarget::Lexical(binding) => (
            1,
            binding.source_path.as_str(),
            binding.declaration_span.byte_start,
            binding.declaration_span.byte_end,
            binding.name.as_str(),
            Some(binding.symbol_id.as_str()),
        ),
    }
}

fn same_target(left: &DartDefinitionTarget, right: &DartDefinitionTarget) -> bool {
    match (left, right) {
        (DartDefinitionTarget::Namespace(left), DartDefinitionTarget::Namespace(right)) => {
            match (left.symbol_id.as_deref(), right.symbol_id.as_deref()) {
                (Some(left), Some(right)) => left == right,
                _ => {
                    left.name == right.name
                        && left.kind == right.kind
                        && left.declaration_path == right.declaration_path
                        && left.declaration_span == right.declaration_span
                }
            }
        }
        (DartDefinitionTarget::Lexical(left), DartDefinitionTarget::Lexical(right)) => {
            left.symbol_id == right.symbol_id
        }
        _ => false,
    }
}

fn sort_references(references: &mut [DartIdentifierReference]) {
    references.sort_by(compare_references);
}

fn compare_references(left: &DartIdentifierReference, right: &DartIdentifierReference) -> Ordering {
    (
        &left.source_path,
        left.span.byte_start,
        left.span.byte_end,
        left.kind,
        &left.name,
        &left.prefix,
    )
        .cmp(&(
            &right.source_path,
            right.span.byte_start,
            right.span.byte_end,
            right.kind,
            &right.name,
            &right.prefix,
        ))
}
