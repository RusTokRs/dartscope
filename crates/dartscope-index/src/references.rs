use dartscope_core::{
    DartIdentifierReference, DartIdentifierReferenceKind, DartIdentifierReferenceResolution,
    DartIdentifierReferenceResolutionAnalysis, DartProjectAnalysis, DartProjectReferenceAnalysis,
    DartSymbolQuery,
};

use crate::namespace::{NamespaceResolver, resolve_symbol_with_resolver};
use crate::uri_graph::DartIndexOptions;

/// Resolves a batch of parser-produced namespace identifier references.
///
/// Lexical variable read/write facts are deliberately excluded because they require the
/// parser-produced binding intervals carried by `DartProjectReferenceAnalysis`.
pub fn resolve_identifier_references(
    project: &DartProjectAnalysis,
    references: &[DartIdentifierReference],
) -> DartIdentifierReferenceResolutionAnalysis {
    resolve_identifier_references_with_options(project, references, &DartIndexOptions::default())
}

/// Resolves a batch of namespace references with an explicit conditional-import environment.
pub fn resolve_identifier_references_with_options(
    project: &DartProjectAnalysis,
    references: &[DartIdentifierReference],
    options: &DartIndexOptions,
) -> DartIdentifierReferenceResolutionAnalysis {
    let resolver = NamespaceResolver::new(project, options);
    let mut ordered: Vec<_> = references
        .iter()
        .filter(|reference| {
            !matches!(
                reference.kind,
                DartIdentifierReferenceKind::VariableRead
                    | DartIdentifierReferenceKind::VariableWrite
                    | DartIdentifierReferenceKind::MemberDeclarationInstance
                    | DartIdentifierReferenceKind::MemberDeclarationStatic
                    | DartIdentifierReferenceKind::MemberInvocationInstance
                    | DartIdentifierReferenceKind::MemberInvocationStatic
                    | DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
                    | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
                    | DartIdentifierReferenceKind::MemberPropertyReadInstance
                    | DartIdentifierReferenceKind::MemberPropertyReadStatic
                    | DartIdentifierReferenceKind::MemberPropertyWriteInstance
                    | DartIdentifierReferenceKind::MemberPropertyWriteStatic
            )
        })
        .cloned()
        .collect();
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

/// Resolves all namespace references carried by an opt-in project reference analysis.
pub fn resolve_project_identifier_references(
    analysis: &DartProjectReferenceAnalysis,
) -> DartIdentifierReferenceResolutionAnalysis {
    resolve_identifier_references(&analysis.project, &analysis.references)
}

/// Resolves all project namespace references with an explicit conditional-import environment.
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
