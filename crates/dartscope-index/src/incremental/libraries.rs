//! Per-library path caches and import/export dependency fingerprints.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use dartscope_core::{
    DartPartLinkAnalysis, DartPartLinkStatus, DartProjectAnalysis, DartUriGraph,
    DartUriReferenceKind,
};

use crate::namespace::LibraryMembership;
use crate::uri_graph::sort_uri_references;

use super::DartLibraryDependencyFingerprint;

pub(super) fn library_related_paths(
    changed_paths: &BTreeSet<String>,
    old_links: &DartPartLinkAnalysis,
    new_links: &DartPartLinkAnalysis,
) -> BTreeSet<String> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for link in old_links.links.iter().chain(&new_links.links) {
        if link.status != DartPartLinkStatus::Matched {
            continue;
        }
        let Some(part_path) = link.part_path.as_ref() else {
            continue;
        };
        adjacency
            .entry(link.owner_path.clone())
            .or_default()
            .insert(part_path.clone());
        adjacency
            .entry(part_path.clone())
            .or_default()
            .insert(link.owner_path.clone());
    }

    let mut visited = changed_paths.clone();
    let mut related = BTreeSet::new();
    let mut queue: VecDeque<_> = changed_paths.iter().cloned().collect();
    while let Some(path) = queue.pop_front() {
        let Some(neighbors) = adjacency.get(&path) else {
            continue;
        };
        for neighbor in neighbors {
            if visited.insert(neighbor.clone()) {
                related.insert(neighbor.clone());
                queue.push_back(neighbor.clone());
            }
        }
    }
    related
}

fn grouped_library_paths(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
) -> BTreeMap<String, Vec<String>> {
    let membership = LibraryMembership::from_part_links(part_links);
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &project.files {
        grouped
            .entry(membership.owner_of(&file.path).to_string())
            .or_default()
            .push(file.path.clone());
    }
    for paths in grouped.values_mut() {
        paths.sort();
        paths.dedup();
    }
    grouped
}

pub(super) fn build_library_path_cache(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
) -> BTreeMap<String, Arc<Vec<String>>> {
    grouped_library_paths(project, part_links)
        .into_iter()
        .map(|(owner, paths)| (owner, Arc::new(paths)))
        .collect()
}

pub(super) fn refresh_library_path_cache(
    project: &DartProjectAnalysis,
    part_links: &DartPartLinkAnalysis,
    cache: &mut BTreeMap<String, Arc<Vec<String>>>,
) -> u64 {
    let grouped = grouped_library_paths(project, part_links);
    let before = cache.len();
    cache.retain(|owner, _| grouped.contains_key(owner));
    let mut rebuilt = (before - cache.len()) as u64;
    for (owner, paths) in grouped {
        if cache
            .get(&owner)
            .is_some_and(|existing| existing.as_ref() == &paths)
        {
            continue;
        }
        cache.insert(owner, Arc::new(paths));
        rebuilt += 1;
    }
    rebuilt
}

pub(super) fn build_library_dependency_fingerprint_cache(
    uri_graph: &DartUriGraph,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> BTreeMap<String, Arc<DartLibraryDependencyFingerprint>> {
    library_paths
        .iter()
        .map(|(owner, paths)| {
            (
                owner.clone(),
                Arc::new(library_dependency_fingerprint(owner, paths, uri_graph)),
            )
        })
        .collect()
}

fn library_dependency_fingerprint(
    owner: &str,
    member_paths: &[String],
    uri_graph: &DartUriGraph,
) -> DartLibraryDependencyFingerprint {
    let members: BTreeSet<_> = member_paths.iter().map(String::as_str).collect();
    let mut references: Vec<_> = uri_graph
        .references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartUriReferenceKind::Import | DartUriReferenceKind::Export
            ) && members.contains(reference.source_path.as_str())
        })
        .cloned()
        .collect();
    sort_uri_references(&mut references);
    DartLibraryDependencyFingerprint {
        owner_path: owner.to_string(),
        member_paths: member_paths.to_vec(),
        references,
    }
}

pub(super) fn refresh_library_dependency_fingerprint_cache(
    uri_graph: &DartUriGraph,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
    cache: &mut BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
) -> u64 {
    let desired = build_library_dependency_fingerprint_cache(uri_graph, library_paths);
    let before = cache.len();
    cache.retain(|owner, _| desired.contains_key(owner));
    let mut rebuilt = (before - cache.len()) as u64;
    for (owner, fingerprint) in desired {
        if cache
            .get(&owner)
            .is_some_and(|existing| existing.as_ref() == fingerprint.as_ref())
        {
            continue;
        }
        cache.insert(owner, fingerprint);
        rebuilt += 1;
    }
    rebuilt
}

pub(super) fn aggregate_library_dependency_fingerprints(
    cache: &BTreeMap<String, Arc<DartLibraryDependencyFingerprint>>,
) -> Vec<DartLibraryDependencyFingerprint> {
    cache
        .values()
        .map(|fingerprint| fingerprint.as_ref().clone())
        .collect()
}

pub(super) fn affected_library_owners(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    old_project: &DartProjectAnalysis,
    old_part_links: &DartPartLinkAnalysis,
    new_project: &DartProjectAnalysis,
    new_part_links: &DartPartLinkAnalysis,
) -> Vec<String> {
    let old_files: BTreeSet<_> = old_project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let new_files: BTreeSet<_> = new_project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let old_membership = LibraryMembership::from_part_links(old_part_links);
    let new_membership = LibraryMembership::from_part_links(new_part_links);
    let mut owners = BTreeSet::new();
    for path in changed_paths.iter().chain(affected_paths) {
        if old_files.contains(path.as_str()) {
            owners.insert(old_membership.owner_of(path).to_string());
        }
        if new_files.contains(path.as_str()) {
            owners.insert(new_membership.owner_of(path).to_string());
        }
    }
    owners.into_iter().collect()
}

pub(super) fn graphql_library_owners(
    project: &DartProjectAnalysis,
    library_paths: &BTreeMap<String, Arc<Vec<String>>>,
) -> BTreeSet<String> {
    let use_paths: BTreeSet<_> = project
        .files
        .iter()
        .filter(|file| !file.graphql_operation_uses.is_empty())
        .map(|file| file.path.as_str())
        .collect();
    library_paths
        .iter()
        .filter(|(_, paths)| paths.iter().any(|path| use_paths.contains(path.as_str())))
        .map(|(owner, _)| owner.clone())
        .collect()
}
