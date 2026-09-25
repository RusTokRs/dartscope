use dartscope_core::{
    DartDeclarationKind, DartIdentifierReference, DartIdentifierReferenceKind,
    DartProjectReferenceAnalysis, DartSymbolCandidate, DartSymbolQuery, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus, DartUriGraph,
};

use crate::namespace::{NamespaceResolver, resolve_member_owner_with_resolver};

use super::{
    DartDefinitionResolutionStatus, DartDefinitionTarget, ResolvedReference, combine_statuses,
    compare_targets, definition_status, external_namespace_uris, refine_constructor_target,
    same_target,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum MemberFamily {
    Method,
    Property,
    Operator,
}

impl MemberFamily {
    fn contains(self, kind: DartDeclarationKind) -> bool {
        match self {
            Self::Method => kind == DartDeclarationKind::Method,
            Self::Property => matches!(
                kind,
                DartDeclarationKind::Field
                    | DartDeclarationKind::Getter
                    | DartDeclarationKind::Setter
            ),
            Self::Operator => kind == DartDeclarationKind::Operator,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum MemberUse {
    Call,
    Read,
    Write,
    Operator,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct IndexedMember {
    owner_symbol_id: String,
    is_static: bool,
    candidate: DartSymbolCandidate,
}

#[derive(Debug, Clone, Default)]
pub(super) struct MemberIndex {
    members: Vec<IndexedMember>,
}

impl MemberIndex {
    pub(super) fn new(analysis: &DartProjectReferenceAnalysis) -> Self {
        let mut members = analysis
            .references
            .iter()
            .filter_map(|reference| {
                let (family, is_static) = declaration_fact(reference.kind)?;
                let owner_symbol_id = reference.prefix.clone()?;
                let file = analysis
                    .project
                    .files
                    .iter()
                    .find(|file| file.path == reference.source_path)?;
                let declaration = file.declarations.iter().find(|declaration| {
                    family.contains(declaration.kind)
                        && declaration.name == reference.name
                        && declaration.parent_symbol_id.as_deref() == Some(owner_symbol_id.as_str())
                        && declaration_span_contains(declaration, &reference.span)
                })?;
                Some(IndexedMember {
                    owner_symbol_id,
                    is_static,
                    candidate: declaration_candidate(
                        file.path.as_str(),
                        declaration,
                        DartSymbolResolutionBasis::SameFile,
                    ),
                })
            })
            .collect::<Vec<_>>();
        members.sort_by(|left, right| {
            (
                &left.owner_symbol_id,
                left.is_static,
                &left.candidate.declaration_path,
                left.candidate.declaration_span.byte_start,
                &left.candidate.name,
                left.candidate.kind,
            )
                .cmp(&(
                    &right.owner_symbol_id,
                    right.is_static,
                    &right.candidate.declaration_path,
                    right.candidate.declaration_span.byte_start,
                    &right.candidate.name,
                    right.candidate.kind,
                ))
        });
        members.dedup();
        Self { members }
    }
}

pub(super) fn resolve_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> Option<ResolvedReference> {
    if declaration_fact(reference.kind).is_some() {
        return Some(resolve_declaration_reference(member_index, reference));
    }
    let (member_use, is_static) = access_fact(reference.kind)?;
    Some(if is_static {
        resolve_static_reference(
            analysis,
            namespace,
            uri_graph,
            member_index,
            reference,
            member_use,
        )
    } else {
        resolve_instance_reference(analysis, namespace, member_index, reference, member_use)
    })
}

pub(super) fn is_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {
    declaration_fact(kind).is_some()
}

fn declaration_fact(kind: DartIdentifierReferenceKind) -> Option<(MemberFamily, bool)> {
    match kind {
        DartIdentifierReferenceKind::MemberDeclarationInstance => {
            Some((MemberFamily::Method, false))
        }
        DartIdentifierReferenceKind::MemberDeclarationStatic => Some((MemberFamily::Method, true)),
        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance => {
            Some((MemberFamily::Property, false))
        }
        DartIdentifierReferenceKind::MemberPropertyDeclarationStatic => {
            Some((MemberFamily::Property, true))
        }
        DartIdentifierReferenceKind::MemberOperatorDeclaration => {
            Some((MemberFamily::Operator, false))
        }
        _ => None,
    }
}

fn access_fact(kind: DartIdentifierReferenceKind) -> Option<(MemberUse, bool)> {
    match kind {
        DartIdentifierReferenceKind::MemberInvocationInstance => Some((MemberUse::Call, false)),
        DartIdentifierReferenceKind::MemberInvocationStatic => Some((MemberUse::Call, true)),
        DartIdentifierReferenceKind::MemberPropertyReadInstance => Some((MemberUse::Read, false)),
        DartIdentifierReferenceKind::MemberPropertyReadStatic => Some((MemberUse::Read, true)),
        DartIdentifierReferenceKind::MemberPropertyWriteInstance => Some((MemberUse::Write, false)),
        DartIdentifierReferenceKind::MemberPropertyWriteStatic => Some((MemberUse::Write, true)),
        DartIdentifierReferenceKind::MemberOperatorInvocationInstance => {
            Some((MemberUse::Operator, false))
        }
        _ => None,
    }
}

fn resolve_declaration_reference(
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let (_, is_static) = declaration_fact(reference.kind)
        .expect("member declaration resolver received an access fact");
    let owner_symbol_id = reference.prefix.as_deref();
    let mut targets = member_index
        .members
        .iter()
        .filter(|member| {
            Some(member.owner_symbol_id.as_str()) == owner_symbol_id
                && member.is_static == is_static
                && member.candidate.name == reference.name
                && member.candidate.declaration_path == reference.source_path
                && member.candidate.declaration_span.byte_start <= reference.span.byte_start
                && reference.span.byte_end <= member.candidate.declaration_span.byte_end
        })
        .map(|member| DartDefinitionTarget::Namespace(member.candidate.clone()))
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    let status = match targets.len() {
        0 => DartDefinitionResolutionStatus::Missing,
        1 => DartDefinitionResolutionStatus::Resolved,
        _ => DartDefinitionResolutionStatus::Ambiguous,
    };
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris: Vec::new(),
    }
}

fn resolve_instance_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
    member_use: MemberUse,
) -> ResolvedReference {
    let owner_symbol_id = reference.prefix.as_deref().unwrap_or_default();
    let mut owners = member_owner_candidates_by_symbol_id(
        analysis,
        namespace,
        &reference.source_path,
        owner_symbol_id,
    );
    owners.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
            ))
    });
    owners.dedup();
    let mut refinements = owners
        .iter()
        .map(|owner| {
            refine_instance_member_with_inheritance(
                analysis,
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                member_use,
            )
        })
        .collect::<Vec<_>>();
    if refinements.is_empty()
        || refinements
            .iter()
            .all(|refinement| refinement.status == DartDefinitionResolutionStatus::Missing)
    {
        if let Some(extension) = refine_extension_member(
            analysis,
            member_index,
            namespace,
            &reference.source_path,
            &reference.name,
            member_use,
        ) {
            refinements.push(extension);
        }
    }
    finish_resolution(reference, refinements, Vec::new())
}

fn is_exact_owner_symbol_id(value: &str) -> bool {
    value.contains("::")
}

/// Resolves a static member fact whose owner is carried as an exact symbol ID.
///
/// Unqualified static spellings inside the declaring type cannot name their owner lexically, so the
/// parser records the exact enclosing owner symbol ID instead. Resolution then uses the same exact
/// owner evidence as the instance path and only accepts directly declared static candidates.
fn resolve_exact_owner_static_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
    member_use: MemberUse,
) -> ResolvedReference {
    let owner_symbol_id = reference.prefix.as_deref().unwrap_or_default();
    let mut owners = member_owner_candidates_by_symbol_id(
        analysis,
        namespace,
        &reference.source_path,
        owner_symbol_id,
    );
    owners.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
            ))
    });
    owners.dedup();
    let refinements = owners
        .iter()
        .map(|owner| {
            refine_direct_member(
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                true,
                member_use,
            )
        })
        .collect::<Vec<_>>();
    finish_resolution(reference, refinements, Vec::new())
}

fn resolve_static_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
    member_use: MemberUse,
) -> ResolvedReference {
    if reference
        .prefix
        .as_deref()
        .is_some_and(is_exact_owner_symbol_id)
    {
        return resolve_exact_owner_static_reference(
            analysis,
            namespace,
            member_index,
            reference,
            member_use,
        );
    }
    let Some((import_prefix, owner_name)) = static_member_owner(&reference) else {
        return ResolvedReference {
            reference,
            status: DartDefinitionResolutionStatus::Missing,
            targets: Vec::new(),
            external_uris: Vec::new(),
        };
    };
    let query = DartSymbolQuery {
        source_path: reference.source_path.clone(),
        name: owner_name.clone(),
        prefix: import_prefix.clone(),
    };
    let resolution = resolve_member_owner_with_resolver(&analysis.project, query, namespace);
    let external_uris = external_member_owner_uris(
        analysis,
        uri_graph,
        &reference,
        owner_name.as_str(),
        import_prefix,
    );
    let base_status = if resolution.status
        == DartSymbolResolutionStatus::ConditionalEnvironmentRequired
        && resolution.candidates.is_empty()
        && !external_uris.is_empty()
    {
        DartDefinitionResolutionStatus::ExternalUnindexed
    } else {
        definition_status(resolution.status, !external_uris.is_empty())
    };
    let refinements = resolution
        .candidates
        .iter()
        .map(|owner| {
            refine_static_member(
                analysis,
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                member_use,
            )
        })
        .collect::<Vec<_>>();
    if base_status == DartDefinitionResolutionStatus::Resolved {
        finish_resolution(reference, refinements, external_uris)
    } else {
        let mut targets = refinements
            .iter()
            .flat_map(|refinement| refinement.targets.iter().cloned())
            .collect::<Vec<_>>();
        targets.sort_by(compare_targets);
        targets.dedup_by(|left, right| same_target(left, right));
        ResolvedReference {
            reference,
            status: base_status,
            targets,
            external_uris,
        }
    }
}

#[derive(Debug)]
struct MemberRefinement {
    status: DartDefinitionResolutionStatus,
    targets: Vec<DartDefinitionTarget>,
}

fn refine_static_member(
    analysis: &DartProjectReferenceAnalysis,
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    member_name: &str,
    member_use: MemberUse,
) -> MemberRefinement {
    let direct = refine_direct_member(
        member_index,
        namespace,
        source_path,
        owner,
        member_name,
        true,
        member_use,
    );
    if direct.status != DartDefinitionResolutionStatus::Missing
        || !matches!(member_use, MemberUse::Call | MemberUse::Read)
        || !matches!(
            owner.kind,
            DartDeclarationKind::Class | DartDeclarationKind::ExtensionType
        )
    {
        return direct;
    }
    let constructor_name = if member_name == "new" {
        owner.name.clone()
    } else {
        format!("{}.{member_name}", owner.name)
    };
    let constructor =
        refine_constructor_target(analysis, namespace, source_path, owner, &constructor_name);
    if constructor.status == DartDefinitionResolutionStatus::Missing {
        direct
    } else {
        MemberRefinement {
            status: constructor.status,
            targets: constructor.targets,
        }
    }
}

fn refine_direct_member(
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    member_name: &str,
    is_static: bool,
    member_use: MemberUse,
) -> MemberRefinement {
    let Some(owner_symbol_id) = owner.symbol_id.as_deref() else {
        return missing_target(owner);
    };
    let mut exact = member_index
        .members
        .iter()
        .filter(|member| {
            member.owner_symbol_id == owner_symbol_id
                && member.is_static == is_static
                && member.candidate.name == member_name
                && candidate_matches_use(member.candidate.kind, member_use)
        })
        .map(|member| {
            let mut candidate = member.candidate.clone();
            candidate.basis = owner.basis;
            candidate
        })
        .collect::<Vec<_>>();
    if exact.is_empty() {
        return missing_target(owner);
    }
    let visible = !member_name.starts_with('_')
        || exact
            .iter()
            .all(|candidate| namespace.same_library(source_path, &candidate.declaration_path));
    if !visible {
        for candidate in &mut exact {
            candidate.basis = DartSymbolResolutionBasis::NotVisible;
        }
    }
    exact.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
            left.kind,
            &left.symbol_id,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
                right.kind,
                &right.symbol_id,
            ))
    });
    exact.dedup();
    let status = if !visible {
        DartDefinitionResolutionStatus::NotVisible
    } else if exact.len() == 1 {
        DartDefinitionResolutionStatus::Resolved
    } else {
        DartDefinitionResolutionStatus::Ambiguous
    };
    MemberRefinement {
        status,
        targets: exact
            .into_iter()
            .map(DartDefinitionTarget::Namespace)
            .collect(),
    }
}

fn refine_instance_member_with_inheritance(
    analysis: &DartProjectReferenceAnalysis,
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    member_name: &str,
    member_use: MemberUse,
) -> MemberRefinement {
    let direct = refine_direct_member(
        member_index,
        namespace,
        source_path,
        owner,
        member_name,
        false,
        member_use,
    );
    if direct.status != DartDefinitionResolutionStatus::Missing {
        return direct;
    }
    if let Some(inherited) = refine_inherited_instance_member(
        analysis,
        member_index,
        namespace,
        source_path,
        owner,
        member_name,
        member_use,
    ) {
        return inherited;
    }
    if let Some(extension) = refine_extension_member(
        analysis,
        member_index,
        namespace,
        source_path,
        member_name,
        member_use,
    ) {
        return extension;
    }
    direct
}

fn refine_inherited_instance_member(
    analysis: &DartProjectReferenceAnalysis,
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    member_name: &str,
    member_use: MemberUse,
) -> Option<MemberRefinement> {
    let owner_symbol_id = owner.symbol_id.as_deref()?;
    let owner_decl = find_declaration_by_symbol_id(analysis, owner_symbol_id)?;
    let mut ancestors: Vec<String> = Vec::new();
    if let Some(extends) = &owner_decl.extends {
        ancestors.push(extends.clone());
    }
    ancestors.extend(owner_decl.mixes_in.clone());
    if ancestors.is_empty() {
        return None;
    }
    let mut inherited_targets: Vec<DartSymbolCandidate> = Vec::new();
    let mut inherited_statuses: Vec<DartDefinitionResolutionStatus> = Vec::new();
    for qualified in ancestors {
        let (prefix, name) = split_qualified(&qualified);
        let query = DartSymbolQuery {
            source_path: owner.declaration_path.clone(),
            name,
            prefix,
        };
        let resolution =
            resolve_member_owner_with_resolver(&analysis.project, query, namespace);
        for candidate in resolution.candidates {
            let refinement = refine_direct_member(
                member_index,
                namespace,
                source_path,
                &candidate,
                member_name,
                false,
                member_use,
            );
            if refinement.status != DartDefinitionResolutionStatus::Missing {
                inherited_statuses.push(refinement.status);
                inherited_targets.extend(
                    refinement
                        .targets
                        .iter()
                        .filter_map(|target| match target {
                            DartDefinitionTarget::Namespace(candidate) => Some(candidate.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    if inherited_targets.is_empty() {
        return None;
    }
    inherited_targets.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
            left.kind,
            &left.symbol_id,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
                right.kind,
                &right.symbol_id,
            ))
    });
    inherited_targets.dedup();
    let targets = inherited_targets
        .into_iter()
        .map(DartDefinitionTarget::Namespace)
        .collect::<Vec<_>>();
    let status = combine_statuses(&inherited_statuses, targets.len());
    Some(MemberRefinement { status, targets })
}

fn refine_extension_member(
    analysis: &DartProjectReferenceAnalysis,
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    member_name: &str,
    member_use: MemberUse,
) -> Option<MemberRefinement> {
    let mut extension_candidates: Vec<DartSymbolCandidate> = Vec::new();
    let mut extension_statuses: Vec<DartDefinitionResolutionStatus> = Vec::new();
    for member in &member_index.members {
        if member.is_static {
            continue;
        }
        if member.candidate.name != member_name {
            continue;
        }
        if !candidate_matches_use(member.candidate.kind, member_use) {
            continue;
        }
        let Some(owner_decl) =
            find_declaration_by_symbol_id(analysis, member.owner_symbol_id.as_str())
        else {
            continue;
        };
        if !matches!(
            owner_decl.kind,
            DartDeclarationKind::Extension | DartDeclarationKind::ExtensionType
        ) {
            continue;
        }
        // Without receiver inference we accept any extension with a matching member name.
        // A future `on`-type-aware filter can be added once receiver-type inference is available:
        //   if owner_decl.extends.as_deref() != Some(inferred_receiver) { continue; }
        let is_private = member_name.starts_with('_');
        let same_library =
            namespace.same_library(source_path, &member.candidate.declaration_path);
        let visible = !is_private || same_library;
        let mut candidate = member.candidate.clone();
        if !visible {
            candidate.basis = DartSymbolResolutionBasis::NotVisible;
            extension_statuses.push(DartDefinitionResolutionStatus::NotVisible);
        } else {
            let basis = if member.candidate.declaration_path == source_path {
                DartSymbolResolutionBasis::SameFile
            } else if same_library {
                DartSymbolResolutionBasis::SameLibrary
            } else {
                // Extension from an imported library — conservatively report as DirectImport.
                // Exact import-graph check is deferred; the resolver is intentionally permissive
                // without receiver inference.
                DartSymbolResolutionBasis::DirectImport
            };
            candidate.basis = basis;
            extension_statuses.push(DartDefinitionResolutionStatus::Resolved);
        }
        extension_candidates.push(candidate);
    }
    if extension_candidates.is_empty() {
        return None;
    }
    extension_candidates.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
            left.kind,
            &left.symbol_id,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
                right.kind,
                &right.symbol_id,
            ))
    });
    extension_candidates.dedup();
    let targets = extension_candidates
        .into_iter()
        .map(DartDefinitionTarget::Namespace)
        .collect::<Vec<_>>();
    let status = combine_statuses(&extension_statuses, targets.len());
    Some(MemberRefinement { status, targets })
}

fn find_declaration_by_symbol_id<'a>(
    analysis: &'a DartProjectReferenceAnalysis,
    symbol_id: &str,
) -> Option<&'a dartscope_core::DartDeclaration> {
    for file in &analysis.project.files {
        for declaration in &file.declarations {
            if declaration.symbol_id.as_deref() == Some(symbol_id) {
                return Some(declaration);
            }
        }
    }
    None
}

fn split_qualified(qualified: &str) -> (Option<String>, String) {
    if let Some((head, tail)) = qualified.rsplit_once('.') {
        if head.is_empty() || tail.is_empty() {
            (None, qualified.to_string())
        } else {
            (Some(head.to_string()), tail.to_string())
        }
    } else {
        (None, qualified.to_string())
    }
}

fn candidate_matches_use(kind: DartDeclarationKind, member_use: MemberUse) -> bool {
    match member_use {
        MemberUse::Call | MemberUse::Read => matches!(
            kind,
            DartDeclarationKind::Method | DartDeclarationKind::Field | DartDeclarationKind::Getter
        ),
        MemberUse::Write => matches!(
            kind,
            DartDeclarationKind::Field | DartDeclarationKind::Setter
        ),
        MemberUse::Operator => kind == DartDeclarationKind::Operator,
    }
}

fn missing_target(owner: &DartSymbolCandidate) -> MemberRefinement {
    MemberRefinement {
        status: DartDefinitionResolutionStatus::Missing,
        targets: vec![DartDefinitionTarget::Namespace(owner.clone())],
    }
}

fn finish_resolution(
    reference: DartIdentifierReference,
    refinements: Vec<MemberRefinement>,
    external_uris: Vec<String>,
) -> ResolvedReference {
    let mut targets = refinements
        .iter()
        .flat_map(|refinement| refinement.targets.iter().cloned())
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    let statuses = refinements
        .iter()
        .map(|refinement| refinement.status)
        .collect::<Vec<_>>();
    let status = if statuses.is_empty() {
        DartDefinitionResolutionStatus::Missing
    } else {
        combine_statuses(&statuses, targets.len())
    };
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris,
    }
}

fn static_member_owner(reference: &DartIdentifierReference) -> Option<(Option<String>, String)> {
    let parts = reference.prefix.as_deref()?.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        [owner] if !owner.is_empty() => Some((None, (*owner).to_string())),
        [prefix, owner] if !prefix.is_empty() && !owner.is_empty() => {
            Some((Some((*prefix).to_string()), (*owner).to_string()))
        }
        _ => None,
    }
}

fn member_owner_candidates_by_symbol_id(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner_symbol_id: &str,
) -> Vec<DartSymbolCandidate> {
    let mut owners = Vec::new();
    for file in &analysis.project.files {
        for declaration in &file.declarations {
            if declaration.symbol_id.as_deref() != Some(owner_symbol_id)
                || !is_member_owner_kind(declaration.kind)
            {
                continue;
            }
            let basis = if file.path == source_path {
                DartSymbolResolutionBasis::SameFile
            } else if namespace.same_library(source_path, &file.path) {
                DartSymbolResolutionBasis::SameLibrary
            } else {
                DartSymbolResolutionBasis::NotVisible
            };
            owners.push(declaration_candidate(
                file.path.as_str(),
                declaration,
                basis,
            ));
        }
    }
    owners
}

fn external_member_owner_uris(
    analysis: &DartProjectReferenceAnalysis,
    uri_graph: &DartUriGraph,
    reference: &DartIdentifierReference,
    owner_name: &str,
    import_prefix: Option<String>,
) -> Vec<String> {
    let mut owner_reference = reference.clone();
    owner_reference.name = owner_name.to_string();
    owner_reference.prefix = import_prefix;
    owner_reference.kind = DartIdentifierReferenceKind::InvocationTarget;
    external_namespace_uris(analysis, uri_graph, &owner_reference)
}

fn declaration_candidate(
    path: &str,
    declaration: &dartscope_core::DartDeclaration,
    basis: DartSymbolResolutionBasis,
) -> DartSymbolCandidate {
    DartSymbolCandidate {
        name: declaration.name.clone(),
        kind: declaration.kind,
        symbol_id: declaration.symbol_id.clone(),
        declaration_path: path.to_string(),
        declaration_span: declaration
            .declaration_span
            .clone()
            .unwrap_or_else(|| declaration.span.clone()),
        basis,
    }
}

fn declaration_span_contains(
    declaration: &dartscope_core::DartDeclaration,
    span: &dartscope_core::SourceSpan,
) -> bool {
    let declaration_span = declaration
        .declaration_span
        .as_ref()
        .unwrap_or(&declaration.span);
    declaration_span.byte_start <= span.byte_start && span.byte_end <= declaration_span.byte_end
}

fn is_member_owner_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}
