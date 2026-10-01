//! What an update has to rebuild, and which paths and libraries it affects.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference, DartProjectAnalysis, DartUriGraph};

use super::DartWorkspaceSubsystems;

#[derive(Debug, Clone, Copy)]
pub(super) struct RebuildPlan {
    pub(super) project: bool,
    pub(super) uri_graph: bool,
    pub(super) part_links: bool,
    pub(super) graphql_contracts: bool,
    pub(super) identifier_references: bool,
    pub(super) propagate_dependents: bool,
}

impl RebuildPlan {
    pub(super) const fn all() -> Self {
        Self {
            project: true,
            uri_graph: true,
            part_links: true,
            graphql_contracts: true,
            identifier_references: true,
            propagate_dependents: true,
        }
    }

    pub(super) const fn project_only() -> Self {
        Self {
            project: true,
            uri_graph: false,
            part_links: false,
            graphql_contracts: false,
            identifier_references: false,
            propagate_dependents: false,
        }
    }

    pub(super) const fn metadata(resolution_changed: bool) -> Self {
        Self {
            project: true,
            uri_graph: resolution_changed,
            part_links: resolution_changed,
            graphql_contracts: resolution_changed,
            identifier_references: resolution_changed,
            propagate_dependents: resolution_changed,
        }
    }

    pub(super) const fn options() -> Self {
        Self {
            project: false,
            uri_graph: true,
            part_links: false,
            graphql_contracts: true,
            identifier_references: true,
            propagate_dependents: true,
        }
    }

    pub(super) const fn public(self) -> DartWorkspaceSubsystems {
        DartWorkspaceSubsystems {
            project: self.project,
            uri_graph: self.uri_graph,
            part_links: self.part_links,
            graphql_contracts: self.graphql_contracts,
            identifier_references: self.identifier_references,
        }
    }
}

pub(super) fn file_rebuild_plan(
    old: &DartFileAnalysis,
    new: &DartFileAnalysis,
    references_changed: bool,
) -> RebuildPlan {
    let file_changed = old != new;
    let import_export_changed = old.imports != new.imports || old.exports != new.exports;
    let part_directives_changed = old.parts != new.parts;
    let library_membership_changed = old.library != new.library || old.part_of != new.part_of;
    let namespace_changed =
        import_export_changed || part_directives_changed || library_membership_changed;
    let declarations_changed = cross_file_declarations(old) != cross_file_declarations(new);
    let graphql_operations_changed = old.graphql_operations != new.graphql_operations;

    RebuildPlan {
        project: file_changed,
        uri_graph: import_export_changed || part_directives_changed,
        part_links: part_directives_changed || library_membership_changed,
        graphql_contracts: namespace_changed
            || graphql_operations_changed
            || old.graphql_operation_uses != new.graphql_operation_uses,
        identifier_references: namespace_changed || declarations_changed || references_changed,
        propagate_dependents: namespace_changed
            || declarations_changed
            || graphql_operations_changed,
    }
}

pub(super) fn changed_graphql_operation_names(
    old: Option<&DartFileAnalysis>,
    new: Option<&DartFileAnalysis>,
) -> BTreeSet<String> {
    let old_operations = old
        .map(|file| file.graphql_operations.as_slice())
        .unwrap_or_default();
    let new_operations = new
        .map(|file| file.graphql_operations.as_slice())
        .unwrap_or_default();
    if old_operations == new_operations {
        return BTreeSet::new();
    }
    old_operations
        .iter()
        .chain(new_operations)
        .map(|operation| operation.constant_name.clone())
        .collect()
}

/// The declarations other files can see and keep evidence about: everything but local variables.
///
/// A resolution cached for another file carries the symbol ID, kind and spans of its target, and the
/// target is a member (`b.value`) as often as a top-level declaration. Any difference in these
/// declarations, including a span that moved because of an edit above it or a body that grew below
/// its first line, therefore makes those cached resolutions stale.
fn cross_file_declarations(file: &DartFileAnalysis) -> Vec<&DartDeclaration> {
    file.declarations
        .iter()
        .filter(|declaration| declaration.kind != DartDeclarationKind::LocalVariable)
        .collect()
}

/// The names whose references have to be resolved again after `old` became `new`: those of every
/// cross-file declaration of either text when any of them differs, none otherwise.
pub(super) fn declaration_names_to_refresh(
    old: Option<&DartFileAnalysis>,
    new: Option<&DartFileAnalysis>,
) -> BTreeSet<String> {
    let old_declarations = old.map(cross_file_declarations).unwrap_or_default();
    let new_declarations = new.map(cross_file_declarations).unwrap_or_default();
    if old_declarations == new_declarations {
        return BTreeSet::new();
    }
    old_declarations
        .iter()
        .chain(&new_declarations)
        .map(|declaration| declaration.name.clone())
        .collect()
}

pub(super) fn reference_sources_for_declaration_names(
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    names: &BTreeSet<String>,
) -> BTreeSet<String> {
    if names.is_empty() {
        return BTreeSet::new();
    }
    references_by_path
        .iter()
        .filter(|(_, references)| {
            references
                .iter()
                .any(|reference| names.contains(&reference.name))
        })
        .map(|(path, _)| path.clone())
        .collect()
}

pub(super) fn affected_paths(
    changed_paths: &BTreeSet<String>,
    old_graph: &DartUriGraph,
    new_graph: &DartUriGraph,
    project: &DartProjectAnalysis,
    global_invalidation: bool,
    dependency_impact: bool,
) -> Vec<String> {
    if global_invalidation {
        return project.files.iter().map(|file| file.path.clone()).collect();
    }
    if !dependency_impact {
        return changed_paths.iter().cloned().collect();
    }

    let mut reverse = reverse_dependencies(old_graph);
    for (target, sources) in reverse_dependencies(new_graph) {
        reverse.entry(target).or_default().extend(sources);
    }
    let mut affected = changed_paths.clone();
    let mut queue: VecDeque<_> = changed_paths.iter().cloned().collect();
    while let Some(target) = queue.pop_front() {
        let Some(sources) = reverse.get(&target) else {
            continue;
        };
        for source in sources {
            if affected.insert(source.clone()) {
                queue.push_back(source.clone());
            }
        }
    }
    affected.into_iter().collect()
}

pub(super) fn reverse_dependencies(graph: &DartUriGraph) -> BTreeMap<String, BTreeSet<String>> {
    let mut reverse = BTreeMap::new();
    for reference in &graph.references {
        if let Some(target) = &reference.target_path {
            reverse
                .entry(target.clone())
                .or_insert_with(BTreeSet::new)
                .insert(reference.source_path.clone());
        }
        for candidate in &reference.candidate_paths {
            reverse
                .entry(candidate.clone())
                .or_insert_with(BTreeSet::new)
                .insert(reference.source_path.clone());
        }
    }
    reverse
}
