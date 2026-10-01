//! The per-file URI-reference and reference-resolution caches.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dartscope_core::{
    DartIdentifierReference, DartIdentifierReferenceResolution,
    DartIdentifierReferenceResolutionAnalysis, DartProjectAnalysis, DartUriGraph, DartUriReference,
};

use crate::references::resolve_identifier_references_with_options;
use crate::uri_graph::{DartIndexOptions, UriGraphBuilder, sort_uri_references};

use super::plan::reverse_dependencies;

pub(super) fn build_uri_reference_cache(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> (BTreeMap<String, Arc<Vec<DartUriReference>>>, DartUriGraph) {
    let builder = UriGraphBuilder::new(project, options);
    let cache = project
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                Arc::new(builder.references_for_file(file)),
            )
        })
        .collect();
    let graph = aggregate_uri_graph(&cache);
    (cache, graph)
}

pub(super) fn aggregate_uri_graph(
    cache: &BTreeMap<String, Arc<Vec<DartUriReference>>>,
) -> DartUriGraph {
    let mut references: Vec<_> = cache
        .values()
        .flat_map(|references| references.iter().cloned())
        .collect();
    sort_uri_references(&mut references);
    DartUriGraph { references }
}

pub(super) fn build_reference_resolution_cache(
    project: &DartProjectAnalysis,
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    options: &DartIndexOptions,
) -> (
    BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
    DartIdentifierReferenceResolutionAnalysis,
) {
    let cache = references_by_path
        .iter()
        .map(|(path, references)| {
            let analysis = resolve_identifier_references_with_options(project, references, options);
            (path.clone(), Arc::new(analysis.resolutions))
        })
        .collect();
    let analysis = aggregate_reference_resolutions(&cache);
    (cache, analysis)
}

pub(super) fn aggregate_reference_resolutions(
    cache: &BTreeMap<String, Arc<Vec<DartIdentifierReferenceResolution>>>,
) -> DartIdentifierReferenceResolutionAnalysis {
    DartIdentifierReferenceResolutionAnalysis {
        resolutions: cache
            .values()
            .flat_map(|resolutions| resolutions.iter().cloned())
            .collect(),
    }
}

pub(super) fn uri_rebuild_paths(
    changed_paths: &BTreeSet<String>,
    old_graph: &DartUriGraph,
    project: &DartProjectAnalysis,
    global_invalidation: bool,
    file_set_changed: bool,
) -> BTreeSet<String> {
    if global_invalidation {
        return project.files.iter().map(|file| file.path.clone()).collect();
    }

    let known_files: BTreeSet<_> = project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut paths: BTreeSet<_> = changed_paths
        .iter()
        .filter(|path| known_files.contains(path.as_str()))
        .cloned()
        .collect();
    if file_set_changed {
        let reverse = reverse_dependencies(old_graph);
        for changed in changed_paths {
            if let Some(sources) = reverse.get(changed) {
                paths.extend(sources.iter().cloned());
            }
        }
    }
    paths
}

pub(super) fn reference_rebuild_paths(
    changed_paths: &BTreeSet<String>,
    affected_paths: &[String],
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
    global_invalidation: bool,
    propagate_dependents: bool,
) -> BTreeSet<String> {
    if global_invalidation {
        return references_by_path.keys().cloned().collect();
    }
    let candidates: Box<dyn Iterator<Item = &String> + '_> = if propagate_dependents {
        Box::new(affected_paths.iter())
    } else {
        Box::new(changed_paths.iter())
    };
    candidates
        .filter(|path| references_by_path.contains_key(*path))
        .cloned()
        .collect()
}
