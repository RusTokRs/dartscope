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

#[derive(Debug, Clone, Copy)]
pub(crate) struct NamespaceCandidate<'source> {
    pub(crate) path: &'source str,
    pub(crate) byte_start: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NamespaceCandidateMatch {
    pub(crate) index: usize,
    pub(crate) basis: DartSymbolResolutionBasis,
}

pub(crate) struct NamespaceResolution {
    pub(crate) status: DartSymbolResolutionStatus,
    pub(crate) candidates: Vec<NamespaceCandidateMatch>,
}

struct ImportedCandidateResolution {
    candidates: Vec<NamespaceCandidateMatch>,
    conditional_environment_required: bool,
}

#[derive(Default)]
struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
}

pub(crate) struct NamespaceResolver<'source, 'options> {
    uri_graph: DartUriGraph,
    library_membership: LibraryMembership,
    files_by_path: HashMap<&'source str, &'source DartFileAnalysis>,
    options: &'options DartIndexOptions,
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

impl<'source, 'options> NamespaceResolver<'source, 'options> {
    pub(crate) fn new(
        project: &'source DartProjectAnalysis,
        options: &'options DartIndexOptions,
    ) -> Self {
        Self {
            uri_graph: build_uri_graph_with_options(project, options),
            library_membership: LibraryMembership::from_project(project),
            files_by_path: project
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect(),
            options,
        }
    }

    pub(crate) fn resolve(
        &self,
        source_path: &str,
        name: &str,
        prefix: Option<&str>,
        candidates: &[NamespaceCandidate<'source>],
    ) -> NamespaceResolution {
        let Some(source_file) = self.files_by_path.get(source_path).copied() else {
            return NamespaceResolution {
                status: DartSymbolResolutionStatus::SourceFileMissing,
                candidates: Vec::new(),
            };
        };

        if prefix.is_none() {
            let same_file = candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| {
                    (candidate.path == source_file.path.as_str()).then_some(index)
                })
                .collect();
            if let Some(resolution) =
                finish_local_resolution(candidates, same_file, DartSymbolResolutionBasis::SameFile)
            {
                return resolution;
            }

            let same_library = candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| {
                    (candidate.path != source_file.path.as_str()
                        && self
                            .library_membership
                            .same_library(candidate.path, &source_file.path))
                    .then_some(index)
                })
                .collect();
            if let Some(resolution) = finish_local_resolution(
                candidates,
                same_library,
                DartSymbolResolutionBasis::SameLibrary,
            ) {
                return resolution;
            }
        }

        let namespace_owner = self.library_membership.owner_of(&source_file.path);
        let namespace_file = self
            .files_by_path
            .get(namespace_owner)
            .copied()
            .unwrap_or(source_file);
        let imported = self.imported_candidates(namespace_file, name, prefix, candidates);

        if imported.candidates.len() == 1 {
            NamespaceResolution {
                status: DartSymbolResolutionStatus::Resolved,
                candidates: imported.candidates,
            }
        } else if !imported.candidates.is_empty() {
            NamespaceResolution {
                status: DartSymbolResolutionStatus::Ambiguous,
                candidates: imported.candidates,
            }
        } else {
            let status = if imported.conditional_environment_required {
                DartSymbolResolutionStatus::ConditionalEnvironmentRequired
            } else if candidates.is_empty() {
                DartSymbolResolutionStatus::Missing
            } else {
                DartSymbolResolutionStatus::NotVisible
            };
            let mut matches: Vec<_> = (0..candidates.len())
                .map(|index| NamespaceCandidateMatch {
                    index,
                    basis: DartSymbolResolutionBasis::NotVisible,
                })
                .collect();
            sort_matches(candidates, &mut matches);
            NamespaceResolution {
                status,
                candidates: matches,
            }
        }
    }

    fn imported_candidates(
        &self,
        file: &DartFileAnalysis,
        name: &str,
        prefix: Option<&str>,
        candidates: &[NamespaceCandidate<'source>],
    ) -> ImportedCandidateResolution {
        if name.starts_with('_') {
            return ImportedCandidateResolution {
                candidates: Vec::new(),
                conditional_environment_required: false,
            };
        }

        let mut result = Vec::new();
        let mut conditional_environment_required = false;
        for import in &file.imports {
            if !import_matches_prefix(import, prefix)
                || !namespace_allows_name(&import.combinators, name)
            {
                continue;
            }
            if !import.configurations.is_empty() && self.options.compilation_environment.is_none() {
                conditional_environment_required = true;
                continue;
            }
            let Some(target_path) = resolved_namespace_target(
                &self.uri_graph,
                DartUriReferenceKind::Import,
                &file.path,
                &import.span,
            ) else {
                continue;
            };
            let mut exported = Vec::new();
            self.collect_exported_candidates(
                target_path,
                name,
                candidates,
                &mut HashSet::new(),
                &mut conditional_environment_required,
                &mut exported,
            );
            result.extend(exported.into_iter().map(|index| {
                let basis =
                    if self.library_membership.owner_of(candidates[index].path) == target_path {
                        DartSymbolResolutionBasis::DirectImport
                    } else {
                        DartSymbolResolutionBasis::ReExport
                    };
                NamespaceCandidateMatch { index, basis }
            }));
        }

        sort_matches(candidates, &mut result);
        result.dedup_by(|left, right| {
            let left = candidates[left.index];
            let right = candidates[right.index];
            left.path == right.path && left.byte_start == right.byte_start
        });
        ImportedCandidateResolution {
            candidates: result,
            conditional_environment_required,
        }
    }

    fn collect_exported_candidates(
        &self,
        library_path: &str,
        name: &str,
        candidates: &[NamespaceCandidate<'source>],
        visited: &mut HashSet<String>,
        conditional_environment_required: &mut bool,
        result: &mut Vec<usize>,
    ) {
        if !visited.insert(library_path.to_string())
            || self.library_membership.is_part(library_path)
        {
            return;
        }

        result.extend(
            candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| {
                    (self.library_membership.owner_of(candidate.path) == library_path)
                        .then_some(index)
                }),
        );

        let Some(file) = self.files_by_path.get(library_path) else {
            return;
        };
        for export in &file.exports {
            if !namespace_allows_name(&export.combinators, name) {
                continue;
            }
            if !export.configurations.is_empty() && self.options.compilation_environment.is_none() {
                *conditional_environment_required = true;
                continue;
            }
            if let Some(target_path) = resolved_namespace_target(
                &self.uri_graph,
                DartUriReferenceKind::Export,
                library_path,
                &export.span,
            ) {
                self.collect_exported_candidates(
                    target_path,
                    name,
                    candidates,
                    visited,
                    conditional_environment_required,
                    result,
                );
            }
        }
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
    let resolver = NamespaceResolver::new(project, options);
    resolve_symbol_with_resolver(project, query, &resolver)
}

pub(crate) fn resolve_symbol_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    let declarations = collect_declarations(project, query.name.as_str());
    let candidates: Vec<_> = declarations
        .iter()
        .map(|location| NamespaceCandidate {
            path: location.path,
            byte_start: location.declaration.span.byte_start,
        })
        .collect();
    let resolution = resolver.resolve(
        query.source_path.as_str(),
        query.name.as_str(),
        query.prefix.as_deref(),
        &candidates,
    );
    let mut outputs: Vec<_> = resolution
        .candidates
        .iter()
        .map(|candidate| candidate_output(&declarations[candidate.index], candidate.basis))
        .collect();
    sort_candidates(&mut outputs);
    DartSymbolResolution {
        query,
        status: resolution.status,
        candidates: outputs,
    }
}

fn finish_local_resolution(
    candidates: &[NamespaceCandidate<'_>],
    indices: Vec<usize>,
    basis: DartSymbolResolutionBasis,
) -> Option<NamespaceResolution> {
    if indices.is_empty() {
        return None;
    }

    let status = if indices.len() == 1 {
        DartSymbolResolutionStatus::Resolved
    } else {
        DartSymbolResolutionStatus::Ambiguous
    };
    let mut matches: Vec<_> = indices
        .into_iter()
        .map(|index| NamespaceCandidateMatch { index, basis })
        .collect();
    sort_matches(candidates, &mut matches);
    Some(NamespaceResolution {
        status,
        candidates: matches,
    })
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

fn import_matches_prefix(import: &dartscope_core::DartImport, prefix: Option<&str>) -> bool {
    match prefix {
        Some(prefix) => import.prefix.as_deref() == Some(prefix),
        None => import.prefix.is_none() && !import.is_deferred,
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

fn sort_matches(candidates: &[NamespaceCandidate<'_>], matches: &mut [NamespaceCandidateMatch]) {
    matches.sort_by(|left, right| {
        let left_candidate = candidates[left.index];
        let right_candidate = candidates[right.index];
        (
            left_candidate.path,
            left_candidate.byte_start,
            basis_order(left.basis),
            left.index,
        )
            .cmp(&(
                right_candidate.path,
                right_candidate.byte_start,
                basis_order(right.basis),
                right.index,
            ))
    });
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
