use dartscope_core::{
    DartIdentifierReference, DartIdentifierReferenceResolution,
    DartIdentifierReferenceResolutionAnalysis, DartProjectAnalysis, DartProjectReferenceAnalysis,
    DartSymbolQuery,
};

use crate::namespace::{NamespaceResolver, resolve_symbol_with_resolver};
use crate::uri_graph::DartIndexOptions;

/// Resolves a batch of parser-produced identifier references.
pub fn resolve_identifier_references(
    project: &DartProjectAnalysis,
    references: &[DartIdentifierReference],
) -> DartIdentifierReferenceResolutionAnalysis {
    resolve_identifier_references_with_options(project, references, &DartIndexOptions::default())
}

/// Resolves a batch of references with an explicit conditional-import environment.
pub fn resolve_identifier_references_with_options(
    project: &DartProjectAnalysis,
    references: &[DartIdentifierReference],
    options: &DartIndexOptions,
) -> DartIdentifierReferenceResolutionAnalysis {
    let resolver = NamespaceResolver::new(project, options);
    let mut ordered = references.to_vec();
    sort_references(&mut ordered);
    let resolutions = ordered
        .into_iter()
        .map(|reference| {
            let query = DartSymbolQuery {
                source_path: reference.source_path.clone(),
                name: reference.name.clone(),
                prefix: reference.prefix.clone(),
            };
            let resolution = resolve_symbol_with_resolver(project, query, &resolver);
            DartIdentifierReferenceResolution {
                reference,
                status: resolution.status,
                candidates: resolution.candidates,
            }
        })
        .collect();
    DartIdentifierReferenceResolutionAnalysis { resolutions }
}

/// Resolves all references carried by an opt-in project reference analysis.
pub fn resolve_project_identifier_references(
    analysis: &DartProjectReferenceAnalysis,
) -> DartIdentifierReferenceResolutionAnalysis {
    resolve_identifier_references(&analysis.project, &analysis.references)
}

/// Resolves all project references with an explicit conditional-import environment.
pub fn resolve_project_identifier_references_with_options(
    analysis: &DartProjectReferenceAnalysis,
    options: &DartIndexOptions,
) -> DartIdentifierReferenceResolutionAnalysis {
    resolve_identifier_references_with_options(&analysis.project, &analysis.references, options)
}

fn sort_references(references: &mut [DartIdentifierReference]) {
    references.sort_by(|left, right| {
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
    });
}
