use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartIdentifierReference, DartIdentifierReferenceKind,
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

/// How far up the `extends`/`with`/`on` relations one member lookup climbs. Real hierarchies are a
/// few levels deep; the cap only bounds pathological generated chains.
const MAX_ANCESTOR_DEPTH: usize = 128;

/// The supertypes of one type in member lookup order. Each entry holds the declarations one
/// `extends`/`with`/`on` entry names: one declaration, or several when the name is ambiguous.
type AncestorGroups = Vec<Vec<DartSymbolCandidate>>;

/// Member positions of one owner, by member name, split by staticness.
#[derive(Debug, Clone, Default)]
struct OwnerMembers {
    instance: HashMap<String, Vec<usize>>,
    statics: HashMap<String, Vec<usize>>,
}

/// Every lookup the member resolver needs is a hash lookup: resolving all references of a project
/// must not rescan every member or declaration once per reference.
#[derive(Debug, Clone, Default)]
pub(super) struct MemberIndex {
    members: Vec<IndexedMember>,
    by_owner: HashMap<String, OwnerMembers>,
    instance_by_name: HashMap<String, Vec<usize>>,
    /// `(file position, declaration position)` per symbol ID, in project order.
    declarations: HashMap<String, Vec<(usize, usize)>>,
    /// Ancestors per owner symbol ID. They depend only on the owner, so every reference inside the
    /// same type shares one walk.
    ancestors: RefCell<HashMap<String, Rc<AncestorGroups>>>,
}

impl MemberIndex {
    pub(super) fn new(analysis: &DartProjectReferenceAnalysis) -> Self {
        let mut file_positions: HashMap<&str, usize> = HashMap::new();
        let mut by_parent_and_name: HashMap<(usize, &str, &str), Vec<usize>> = HashMap::new();
        let mut declarations: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        for (file_position, file) in analysis.project.files.iter().enumerate() {
            // The first file with a path wins, as it did when files were searched linearly.
            file_positions.entry(file.path.as_str()).or_insert(file_position);
            for (position, declaration) in file.declarations.iter().enumerate() {
                if let Some(parent) = declaration.parent_symbol_id.as_deref() {
                    by_parent_and_name
                        .entry((file_position, parent, declaration.name.as_str()))
                        .or_default()
                        .push(position);
                }
                if let Some(symbol_id) = declaration.symbol_id.as_deref() {
                    declarations
                        .entry(symbol_id.to_string())
                        .or_default()
                        .push((file_position, position));
                }
            }
        }
        let mut members = analysis
            .references
            .iter()
            .filter_map(|reference| {
                let (family, is_static) = declaration_fact(reference.kind)?;
                let owner_symbol_id = reference.prefix.clone()?;
                let file_position = *file_positions.get(reference.source_path.as_str())?;
                let file = &analysis.project.files[file_position];
                let declaration = by_parent_and_name
                    .get(&(
                        file_position,
                        owner_symbol_id.as_str(),
                        reference.name.as_str(),
                    ))?
                    .iter()
                    .map(|position| &file.declarations[*position])
                    .find(|declaration| {
                        family.contains(declaration.kind)
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
        let mut by_owner: HashMap<String, OwnerMembers> = HashMap::new();
        let mut instance_by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (position, member) in members.iter().enumerate() {
            let owner = by_owner.entry(member.owner_symbol_id.clone()).or_default();
            let named = if member.is_static {
                &mut owner.statics
            } else {
                &mut owner.instance
            };
            named
                .entry(member.candidate.name.clone())
                .or_default()
                .push(position);
            if !member.is_static {
                instance_by_name
                    .entry(member.candidate.name.clone())
                    .or_default()
                    .push(position);
            }
        }
        Self {
            members,
            by_owner,
            instance_by_name,
            declarations,
            ancestors: RefCell::default(),
        }
    }

    /// Members of one owner with one name, in index order.
    fn members_named<'a>(
        &'a self,
        owner_symbol_id: &str,
        is_static: bool,
        name: &str,
    ) -> impl Iterator<Item = &'a IndexedMember> + 'a {
        self.by_owner
            .get(owner_symbol_id)
            .and_then(|owner| {
                if is_static {
                    owner.statics.get(name)
                } else {
                    owner.instance.get(name)
                }
            })
            .into_iter()
            .flatten()
            .map(|position| &self.members[*position])
    }

    /// Instance members with one name, whatever their owner.
    fn instance_members_named<'a>(
        &'a self,
        name: &str,
    ) -> impl Iterator<Item = &'a IndexedMember> + 'a {
        self.instance_by_name
            .get(name)
            .into_iter()
            .flatten()
            .map(|position| &self.members[*position])
    }

    /// Declarations carrying one symbol ID with the path of their file.
    fn declarations_with_symbol_id<'a>(
        &'a self,
        analysis: &'a DartProjectReferenceAnalysis,
        symbol_id: &str,
    ) -> impl Iterator<Item = (&'a str, &'a DartDeclaration)> + 'a {
        self.declarations
            .get(symbol_id)
            .into_iter()
            .flatten()
            .map(|(file_position, position)| {
                let file = &analysis.project.files[*file_position];
                (file.path.as_str(), &file.declarations[*position])
            })
    }

    fn declaration<'a>(
        &'a self,
        analysis: &'a DartProjectReferenceAnalysis,
        symbol_id: &str,
    ) -> Option<&'a DartDeclaration> {
        self.declarations_with_symbol_id(analysis, symbol_id)
            .next()
            .map(|(_, declaration)| declaration)
    }

    /// The supertypes of `owner` in the order Dart looks members up: the mixins of a class (the one
    /// applied last first), then its superclass, then everything those inherit in turn. A mixin's
    /// `on` constraints and an extension's extended type count as supertypes too, because the body
    /// of either sees their members.
    ///
    /// Each type appears once, at its first position, so inheritance cycles end the walk instead of
    /// repeating it, and a chain stops at [`MAX_ANCESTOR_DEPTH`] levels.
    fn ancestor_groups(
        &self,
        analysis: &DartProjectReferenceAnalysis,
        namespace: &NamespaceResolver<'_, '_>,
        owner: &DartSymbolCandidate,
    ) -> Rc<AncestorGroups> {
        let Some(owner_symbol_id) = owner.symbol_id.as_deref() else {
            return Rc::default();
        };
        if let Some(groups) = self.ancestors.borrow().get(owner_symbol_id) {
            return Rc::clone(groups);
        }
        let groups = Rc::new(self.walk_ancestors(analysis, namespace, owner, owner_symbol_id));
        self.ancestors
            .borrow_mut()
            .insert(owner_symbol_id.to_string(), Rc::clone(&groups));
        groups
    }

    fn walk_ancestors(
        &self,
        analysis: &DartProjectReferenceAnalysis,
        namespace: &NamespaceResolver<'_, '_>,
        owner: &DartSymbolCandidate,
        owner_symbol_id: &str,
    ) -> AncestorGroups {
        let mut visited: HashSet<String> = HashSet::from([owner_symbol_id.to_string()]);
        let mut ancestors: AncestorGroups = Vec::new();
        // An explicit stack keeps a very long chain from overflowing the call stack. Groups are
        // pushed in reverse so the first relation of a type is expanded first.
        let mut pending: Vec<(Vec<DartSymbolCandidate>, usize)> = Vec::new();
        self.push_relations(analysis, namespace, owner, 1, &mut pending);
        while let Some((group, depth)) = pending.pop() {
            let group = group
                .into_iter()
                .filter(|candidate| {
                    candidate
                        .symbol_id
                        .as_deref()
                        .is_some_and(|symbol_id| visited.insert(symbol_id.to_string()))
                })
                .collect::<Vec<_>>();
            if group.is_empty() {
                continue;
            }
            if depth < MAX_ANCESTOR_DEPTH {
                for candidate in group.iter().rev() {
                    self.push_relations(analysis, namespace, candidate, depth + 1, &mut pending);
                }
            }
            ancestors.push(group);
        }
        ancestors
    }

    fn push_relations(
        &self,
        analysis: &DartProjectReferenceAnalysis,
        namespace: &NamespaceResolver<'_, '_>,
        subtype: &DartSymbolCandidate,
        depth: usize,
        pending: &mut Vec<(Vec<DartSymbolCandidate>, usize)>,
    ) {
        let Some(declaration) = subtype
            .symbol_id
            .as_deref()
            .and_then(|symbol_id| self.declaration(analysis, symbol_id))
        else {
            return;
        };
        let relations = declaration
            .mixes_in
            .iter()
            .rev()
            .chain(&declaration.extends)
            .chain(&declaration.on_types);
        let groups = relations
            .map(|qualified| {
                let (prefix, name) = split_qualified(qualified);
                let query = DartSymbolQuery {
                    source_path: subtype.declaration_path.clone(),
                    name,
                    prefix,
                };
                let mut candidates = resolve_member_owner_with_resolver(query, namespace).candidates;
                sort_candidates(&mut candidates);
                candidates
            })
            .collect::<Vec<_>>();
        pending.extend(groups.into_iter().rev().map(|group| (group, depth)));
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
    let owner_symbol_id = reference.prefix.as_deref().unwrap_or_default();
    let mut targets = member_index
        .members_named(owner_symbol_id, is_static, &reference.name)
        .filter(|member| {
            member.candidate.declaration_path == reference.source_path
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
    let owners = member_owners_by_symbol_id(
        analysis,
        namespace,
        member_index,
        &reference.source_path,
        owner_symbol_id,
    );
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
    // Only a receiver whose type is not a project declaration can be matched against every visible
    // extension. When the receiver's type is known, an extension has to apply to that type.
    if owners.is_empty()
        && let Some(extension) = refine_extension_member(
            analysis,
            member_index,
            namespace,
            &reference.source_path,
            &reference.name,
            member_use,
            ExtensionReceiver::Unknown,
        )
    {
        refinements.push(extension);
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
    let owners = member_owners_by_symbol_id(
        analysis,
        namespace,
        member_index,
        &reference.source_path,
        owner_symbol_id,
    );
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
    let resolution = resolve_member_owner_with_resolver(query, namespace);
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
        .members_named(owner_symbol_id, is_static, member_name)
        .filter(|member| candidate_matches_use(member.candidate.kind, member_use))
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
    sort_candidates(&mut exact);
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
    let ancestors = member_index.ancestor_groups(analysis, namespace, owner);
    if let Some(inherited) = refine_inherited_instance_member(
        member_index,
        namespace,
        source_path,
        &ancestors,
        member_name,
        member_use,
    ) {
        return inherited;
    }
    // The member is not declared by the type or any of its supertypes, so only an extension of one
    // of those types can provide it.
    let receiver_types = std::iter::once(owner)
        .chain(ancestors.iter().flatten())
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();
    refine_extension_member(
        analysis,
        member_index,
        namespace,
        source_path,
        member_name,
        member_use,
        ExtensionReceiver::Known(&receiver_types),
    )
    .unwrap_or(direct)
}

/// Looks the member up in the supertypes of the owner in lookup order. The first supertype that
/// declares it ends the search, so a member declared higher up never shadows a nearer one. Several
/// declarations behind one ambiguous name make the result ambiguous.
fn refine_inherited_instance_member(
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    ancestors: &AncestorGroups,
    member_name: &str,
    member_use: MemberUse,
) -> Option<MemberRefinement> {
    for group in ancestors {
        let mut inherited_targets: Vec<DartSymbolCandidate> = Vec::new();
        let mut inherited_statuses: Vec<DartDefinitionResolutionStatus> = Vec::new();
        for candidate in group {
            let refinement = refine_direct_member(
                member_index,
                namespace,
                source_path,
                candidate,
                member_name,
                false,
                member_use,
            );
            if refinement.status == DartDefinitionResolutionStatus::Missing {
                continue;
            }
            inherited_statuses.push(refinement.status);
            inherited_targets.extend(refinement.targets.into_iter().filter_map(
                |target| match target {
                    DartDefinitionTarget::Namespace(candidate) => Some(candidate),
                    _ => None,
                },
            ));
        }
        if inherited_targets.is_empty() {
            continue;
        }
        sort_candidates(&mut inherited_targets);
        inherited_targets.dedup();
        let targets = inherited_targets
            .into_iter()
            .map(DartDefinitionTarget::Namespace)
            .collect::<Vec<_>>();
        let status = combine_statuses(&inherited_statuses, targets.len());
        return Some(MemberRefinement { status, targets });
    }
    None
}

/// What is known about the type of the receiver an extension member is looked up for.
#[derive(Debug, Clone, Copy)]
enum ExtensionReceiver<'a> {
    /// The receiver is not a project declaration, so its type, and with it the applicable
    /// extensions, cannot be told apart.
    Unknown,
    /// The receiver is an instance of a project type; these are the names of that type and of every
    /// supertype the project declares.
    Known(&'a [&'a str]),
}

/// Whether the extended type of `extension` includes the receiver. An extension of `Object` or
/// `dynamic` applies to everything, and one without a recorded `on` type (`extension<T> on T`) as
/// well.
fn extension_applies_to(extension: &DartDeclaration, receiver: ExtensionReceiver<'_>) -> bool {
    let ExtensionReceiver::Known(receiver_types) = receiver else {
        return true;
    };
    extension.on_types.is_empty()
        || extension.on_types.iter().any(|on_type| {
            let on_type = on_type.rsplit('.').next().unwrap_or(on_type);
            matches!(on_type, "Object" | "dynamic") || receiver_types.contains(&on_type)
        })
}

/// How the library of `source_path` sees the extension, or `None` when it does not. An extension
/// applies implicitly in its own library and wherever an import that is not deferred brings it into
/// scope, with or without an import prefix. An unnamed extension is only reachable from its own
/// library.
fn extension_visibility(
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    extension: &DartDeclaration,
    extension_path: &str,
) -> Option<DartSymbolResolutionBasis> {
    if extension_path == source_path {
        return Some(DartSymbolResolutionBasis::SameFile);
    }
    if namespace.same_library(source_path, extension_path) {
        return Some(DartSymbolResolutionBasis::SameLibrary);
    }
    if extension.name.is_empty() {
        return None;
    }
    let prefixes = std::iter::once(None).chain(
        namespace
            .import_prefixes(source_path)
            .into_iter()
            .map(|prefix| Some(prefix.to_string())),
    );
    for prefix in prefixes {
        let query = DartSymbolQuery {
            source_path: source_path.to_string(),
            name: extension.name.clone(),
            prefix,
        };
        let visible = resolve_member_owner_with_resolver(query, namespace)
            .candidates
            .into_iter()
            .find(|candidate| {
                candidate.symbol_id == extension.symbol_id
                    && candidate.basis != DartSymbolResolutionBasis::NotVisible
            });
        if let Some(candidate) = visible {
            return Some(candidate.basis);
        }
    }
    None
}

fn refine_extension_member(
    analysis: &DartProjectReferenceAnalysis,
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    member_name: &str,
    member_use: MemberUse,
    receiver: ExtensionReceiver<'_>,
) -> Option<MemberRefinement> {
    let mut extension_candidates: Vec<DartSymbolCandidate> = Vec::new();
    let mut extension_statuses: Vec<DartDefinitionResolutionStatus> = Vec::new();
    for member in member_index.instance_members_named(member_name) {
        if !candidate_matches_use(member.candidate.kind, member_use) {
            continue;
        }
        let Some(extension) = member_index.declaration(analysis, member.owner_symbol_id.as_str())
        else {
            continue;
        };
        // Members of an extension type belong to values of that type, never to other receivers.
        if extension.kind != DartDeclarationKind::Extension
            || !extension_applies_to(extension, receiver)
        {
            continue;
        }
        let same_library = namespace.same_library(source_path, &member.candidate.declaration_path);
        let mut candidate = member.candidate.clone();
        if member_name.starts_with('_') && !same_library {
            candidate.basis = DartSymbolResolutionBasis::NotVisible;
            extension_statuses.push(DartDefinitionResolutionStatus::NotVisible);
        } else if let Some(basis) = extension_visibility(
            namespace,
            source_path,
            extension,
            &member.candidate.declaration_path,
        ) {
            candidate.basis = basis;
            extension_statuses.push(DartDefinitionResolutionStatus::Resolved);
        } else {
            continue;
        }
        extension_candidates.push(candidate);
    }
    if extension_candidates.is_empty() {
        return None;
    }
    sort_candidates(&mut extension_candidates);
    extension_candidates.dedup();
    let targets = extension_candidates
        .into_iter()
        .map(DartDefinitionTarget::Namespace)
        .collect::<Vec<_>>();
    let status = combine_statuses(&extension_statuses, targets.len());
    Some(MemberRefinement { status, targets })
}

fn sort_candidates(candidates: &mut [DartSymbolCandidate]) {
    candidates.sort_by(|left, right| {
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

/// The member-owning declarations carrying one symbol ID, ordered by location.
fn member_owners_by_symbol_id(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    member_index: &MemberIndex,
    source_path: &str,
    owner_symbol_id: &str,
) -> Vec<DartSymbolCandidate> {
    let mut owners = member_index
        .declarations_with_symbol_id(analysis, owner_symbol_id)
        .filter(|(_, declaration)| is_member_owner_kind(declaration.kind))
        .map(|(path, declaration)| {
            let basis = if path == source_path {
                DartSymbolResolutionBasis::SameFile
            } else if namespace.same_library(source_path, path) {
                DartSymbolResolutionBasis::SameLibrary
            } else {
                DartSymbolResolutionBasis::NotVisible
            };
            declaration_candidate(path, declaration, basis)
        })
        .collect::<Vec<_>>();
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
    declaration: &DartDeclaration,
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
    declaration: &DartDeclaration,
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
