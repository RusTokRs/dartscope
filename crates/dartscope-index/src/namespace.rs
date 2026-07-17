use std::collections::{HashMap, HashSet};

use dartscope_core::{
    DartDeclaration, DartFileAnalysis, DartNamespaceCombinatorKind, DartPartLinkStatus,
    DartProjectAnalysis, DartSymbolCandidate, DartSymbolQuery, DartSymbolResolution,
    DartSymbolResolutionBasis, DartSymbolResolutionStatus, DartUriGraph, DartUriReferenceKind,
    DartUriResolution, SourceSpan,
};

use crate::parts::analyze_part_links;
use crate::uri_graph::{DartIndexOptions, build_uri_graph_with_options};

struct DeclarationLocation<'a> {
    path: &'a str,
    declaration: &'a DartDeclaration,
}

struct ImportedDeclarationCandidate<'candidate, 'source> {
    location: &'candidate DeclarationLocation<'source>,
    basis: DartSymbolResolutionBasis,
}

struct ImportedDeclarationResolution<'candidate, 'source> {
    candidates: Vec<ImportedDeclarationCandidate<'candidate, 'source>>,
    conditional_environment_required: bool,
}

#[derive(Default)]
struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
}

struct ExportResolutionContext<'source, 'candidate, 'context> {
    name: &'context str,
    candidates: &'candidate [DeclarationLocation<'source>],
    uri_graph: &'context DartUriGraph,
    files_by_path: &'context HashMap<&'source str, &'source DartFileAnalysis>,
    library_membership: &'context LibraryMembership,
    options: &'context DartIndexOptions,
}

impl LibraryMembership {
    fn from_project(project: &DartProjectAnalysis) -> Self {
        let mut membership = Self::default();
        let mut owners_by_part: HashMap<String, Vec<String>> = HashMap::new();
        for link in analyze_part_links(project)
            .links
            .into_iter()
            .filter(|link| link.status == DartPartLinkStatus::Matched)
        {
            let Some(part_path) = link.part_path else {
                continue;
            };
            owners_by_part
                .entry(part_path)
                .or_default()
                .push(link.owner_path);
        }
        for (part_path, mut owners) in owners_by_part {
            owners.sort();
            owners.dedup();
            let [owner_path] = owners.as_slice() else {
                continue;
            };
            membership
                .owner_by_part
                .insert(part_path, owner_path.clone());
        }
        membership
    }

    fn owner_of<'a>(&'a self, path: &'a str) -> &'a str {
        self.owner_by_part
            .get(path)
            .map(String::as_str)
            .unwrap_or(path)
    }

    fn is_part(&self, path: &str) -> bool {
        self.owner_by_part.contains_key(path)
    }

    fn same_library(&self, left: &str, right: &str) -> bool {
        self.owner_of(left) == self.owner_of(right)
    }
}

/// Resolves one top-level Dart declaration through the source library namespace.
pub fn resolve_symbol(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
) -> DartSymbolResolution {
    resolve_symbol_with_options(project, query, &DartIndexOptions::default())
}

/// Resolves one top-level Dart declaration with an explicit conditional-import environment.
pub fn resolve_symbol_with_options(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    options: &DartIndexOptions,
) -> DartSymbolResolution {
    let files_by_path: HashMap<_, _> = project
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let Some(source_file) = files_by_path.get(query.source_path.as_str()).copied() else {
        return DartSymbolResolution {
            query,
            status: DartSymbolResolutionStatus::SourceFileMissing,
            candidates: Vec::new(),
        };
    };

    let candidates = collect_declarations(project, query.name.as_str());
    let library_membership = LibraryMembership::from_project(project);
    let uri_graph = build_uri_graph_with_options(project, options);

    if query.prefix.is_none() {
        let same_file: Vec<_> = candidates
            .iter()
            .filter(|candidate| candidate.path == source_file.path)
            .collect();
        if let Some(resolution) = finish_local_resolution(
            query.clone(),
            same_file,
            DartSymbolResolutionBasis::SameFile,
        ) {
            return resolution;
        }

        let same_library: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                candidate.path != source_file.path
                    && library_membership.same_library(candidate.path, &source_file.path)
            })
            .collect();
        if let Some(resolution) = finish_local_resolution(
            query.clone(),
            same_library,
            DartSymbolResolutionBasis::SameLibrary,
        ) {
            return resolution;
        }
    }

    let namespace_owner = library_membership.owner_of(&source_file.path);
    let namespace_file = files_by_path
        .get(namespace_owner)
        .copied()
        .unwrap_or(source_file);
    let imported = imported_declaration_candidates(
        namespace_file,
        &query,
        &candidates,
        &uri_graph,
        &files_by_path,
        &library_membership,
        options,
    );
    match imported.candidates.as_slice() {
        [candidate] => {
            return DartSymbolResolution {
                query,
                status: DartSymbolResolutionStatus::Resolved,
                candidates: vec![candidate_output(candidate.location, candidate.basis)],
            };
        }
        candidates if !candidates.is_empty() => {
            let mut outputs: Vec<_> = candidates
                .iter()
                .map(|candidate| candidate_output(candidate.location, candidate.basis))
                .collect();
            sort_candidates(&mut outputs);
            return DartSymbolResolution {
                query,
                status: DartSymbolResolutionStatus::Ambiguous,
                candidates: outputs,
            };
        }
        _ => {}
    }

    let status = if imported.conditional_environment_required {
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired
    } else if candidates.is_empty() {
        DartSymbolResolutionStatus::Missing
    } else {
        DartSymbolResolutionStatus::NotVisible
    };
    let mut outputs: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate_output(candidate, DartSymbolResolutionBasis::NotVisible))
        .collect();
    sort_candidates(&mut outputs);
    DartSymbolResolution {
        query,
        status,
        candidates: outputs,
    }
}

fn finish_local_resolution(
    query: DartSymbolQuery,
    candidates: Vec<&DeclarationLocation<'_>>,
    basis: DartSymbolResolutionBasis,
) -> Option<DartSymbolResolution> {
    match candidates.as_slice() {
        [] => None,
        [candidate] => Some(DartSymbolResolution {
            query,
            status: DartSymbolResolutionStatus::Resolved,
            candidates: vec![candidate_output(candidate, basis)],
        }),
        _ => {
            let mut outputs: Vec<_> = candidates
                .iter()
                .map(|candidate| candidate_output(candidate, basis))
                .collect();
            sort_candidates(&mut outputs);
            Some(DartSymbolResolution {
                query,
                status: DartSymbolResolutionStatus::Ambiguous,
                candidates: outputs,
            })
        }
    }
}

fn collect_declarations<'a>(
    project: &'a DartProjectAnalysis,
    name: &str,
) -> Vec<DeclarationLocation<'a>> {
    let mut candidates = Vec::new();
    for file in &project.files {
        for declaration in &file.declarations {
            if declaration.parent_symbol_id.is_none() && declaration.name == name {
                candidates.push(DeclarationLocation {
                    path: file.path.as_str(),
                    declaration,
                });
            }
        }
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate.path,
            candidate.declaration.span.byte_start,
            candidate.declaration.kind,
        )
    });
    candidates
}

fn imported_declaration_candidates<'candidate, 'source>(
    file: &DartFileAnalysis,
    query: &DartSymbolQuery,
    candidates: &'candidate [DeclarationLocation<'source>],
    uri_graph: &DartUriGraph,
    files_by_path: &HashMap<&'source str, &'source DartFileAnalysis>,
    library_membership: &LibraryMembership,
    options: &DartIndexOptions,
) -> ImportedDeclarationResolution<'candidate, 'source> {
    if query.name.starts_with('_') {
        return ImportedDeclarationResolution {
            candidates: Vec::new(),
            conditional_environment_required: false,
        };
    }

    let context = ExportResolutionContext {
        name: query.name.as_str(),
        candidates,
        uri_graph,
        files_by_path,
        library_membership,
        options,
    };
    let mut result = Vec::new();
    let mut conditional_environment_required = false;
    for import in &file.imports {
        if !import_matches_prefix(import, query.prefix.as_deref())
            || !namespace_allows_name(&import.combinators, query.name.as_str())
        {
            continue;
        }
        if !import.configurations.is_empty() && options.compilation_environment.is_none() {
            conditional_environment_required = true;
            continue;
        }
        let Some(target_path) = resolved_namespace_target(
            uri_graph,
            DartUriReferenceKind::Import,
            &file.path,
            &import.span,
        ) else {
            continue;
        };
        let mut exported = Vec::new();
        collect_exported_declarations(
            target_path,
            &context,
            &mut HashSet::new(),
            &mut conditional_environment_required,
            &mut exported,
        );
        result.extend(
            exported
                .into_iter()
                .map(|location| ImportedDeclarationCandidate {
                    basis: if library_membership.owner_of(location.path) == target_path {
                        DartSymbolResolutionBasis::DirectImport
                    } else {
                        DartSymbolResolutionBasis::ReExport
                    },
                    location,
                }),
        );
    }

    result.sort_by_key(|candidate| {
        (
            candidate.location.path,
            candidate.location.declaration.span.byte_start,
            basis_order(candidate.basis),
        )
    });
    result.dedup_by_key(|candidate| {
        (
            candidate.location.path,
            candidate.location.declaration.span.byte_start,
        )
    });
    ImportedDeclarationResolution {
        candidates: result,
        conditional_environment_required,
    }
}

fn import_matches_prefix(import: &dartscope_core::DartImport, prefix: Option<&str>) -> bool {
    match prefix {
        Some(prefix) => import.prefix.as_deref() == Some(prefix),
        None => import.prefix.is_none() && !import.is_deferred,
    }
}

fn collect_exported_declarations<'source, 'candidate>(
    library_path: &str,
    context: &ExportResolutionContext<'source, 'candidate, '_>,
    visited: &mut HashSet<String>,
    conditional_environment_required: &mut bool,
    result: &mut Vec<&'candidate DeclarationLocation<'source>>,
) {
    if !visited.insert(library_path.to_string()) || context.library_membership.is_part(library_path)
    {
        return;
    }

    result.extend(
        context.candidates.iter().filter(|candidate| {
            context.library_membership.owner_of(candidate.path) == library_path
        }),
    );

    let Some(file) = context.files_by_path.get(library_path) else {
        return;
    };
    for export in &file.exports {
        if !namespace_allows_name(&export.combinators, context.name) {
            continue;
        }
        if !export.configurations.is_empty() && context.options.compilation_environment.is_none() {
            *conditional_environment_required = true;
            continue;
        }
        if let Some(target_path) = resolved_namespace_target(
            context.uri_graph,
            DartUriReferenceKind::Export,
            library_path,
            &export.span,
        ) {
            collect_exported_declarations(
                target_path,
                context,
                visited,
                conditional_environment_required,
                result,
            );
        }
    }
}

fn resolved_namespace_target<'a>(
    uri_graph: &'a DartUriGraph,
    kind: DartUriReferenceKind,
    source_path: &str,
    span: &SourceSpan,
) -> Option<&'a str> {
    uri_graph.references.iter().find_map(|reference| {
        (reference.kind == kind
            && reference.source_path == source_path
            && reference.source_span.byte_start == span.byte_start
            && reference.resolution == DartUriResolution::Resolved)
            .then_some(reference.target_path.as_deref())
            .flatten()
    })
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

fn candidate_output(
    location: &DeclarationLocation<'_>,
    basis: DartSymbolResolutionBasis,
) -> DartSymbolCandidate {
    DartSymbolCandidate {
        name: location.declaration.name.clone(),
        kind: location.declaration.kind,
        symbol_id: location.declaration.symbol_id.clone(),
        declaration_path: location.path.to_string(),
        declaration_span: location
            .declaration
            .declaration_span
            .clone()
            .unwrap_or_else(|| location.declaration.span.clone()),
        basis,
    }
}

fn sort_candidates(candidates: &mut [DartSymbolCandidate]) {
    candidates.sort_by(|left, right| {
        (
            basis_order(left.basis),
            &left.declaration_path,
            left.declaration_span.byte_start,
            left.kind,
            &left.name,
        )
            .cmp(&(
                basis_order(right.basis),
                &right.declaration_path,
                right.declaration_span.byte_start,
                right.kind,
                &right.name,
            ))
    });
}

fn basis_order(basis: DartSymbolResolutionBasis) -> u8 {
    match basis {
        DartSymbolResolutionBasis::SameFile => 0,
        DartSymbolResolutionBasis::SameLibrary => 1,
        DartSymbolResolutionBasis::DirectImport => 2,
        DartSymbolResolutionBasis::ReExport => 3,
        DartSymbolResolutionBasis::NotVisible => 4,
    }
}
