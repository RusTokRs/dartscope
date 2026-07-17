use std::collections::HashMap;

use dartscope_core::{
    DartPartLink, DartPartLinkAnalysis, DartPartLinkStatus, DartPartOfKind, DartProjectAnalysis,
    DartUriReference, DartUriReferenceKind, DartUriResolution,
};

use crate::paths::{normalize_joined_path, parent_path};
use crate::uri_graph::build_uri_graph;

pub fn analyze_part_links(project: &DartProjectAnalysis) -> DartPartLinkAnalysis {
    let uri_graph = build_uri_graph(project);
    analyze_part_links_with_graph(project, &uri_graph)
}

pub(crate) fn analyze_part_links_with_graph(
    project: &DartProjectAnalysis,
    uri_graph: &dartscope_core::DartUriGraph,
) -> DartPartLinkAnalysis {
    let files_by_path: HashMap<_, _> = project
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut links = Vec::new();

    for reference in uri_graph
        .references
        .iter()
        .filter(|reference| reference.kind == DartUriReferenceKind::Part)
    {
        let Some(part_path) = reference.target_path.as_deref() else {
            links.push(part_link_without_target(reference));
            continue;
        };
        let Some(part_file) = files_by_path.get(part_path) else {
            links.push(part_link_without_target(reference));
            continue;
        };
        let Some(part_of) = part_file.part_of.as_ref() else {
            links.push(DartPartLink {
                owner_path: reference.source_path.clone(),
                part_uri: reference.uri.clone(),
                part_path: Some(part_path.to_string()),
                declared_owner: None,
                status: DartPartLinkStatus::MissingPartOf,
                part_span: reference.source_span.clone(),
                part_of_span: None,
            });
            continue;
        };

        let matches_owner = match part_of.kind {
            DartPartOfKind::Uri => {
                normalize_joined_path(&parent_path(part_path), &part_of.library)
                    == reference.source_path
            }
            DartPartOfKind::LibraryName => {
                files_by_path
                    .get(reference.source_path.as_str())
                    .and_then(|owner| owner.library.as_ref())
                    .and_then(|library| library.name.as_deref())
                    == Some(part_of.library.as_str())
            }
        };

        links.push(DartPartLink {
            owner_path: reference.source_path.clone(),
            part_uri: reference.uri.clone(),
            part_path: Some(part_path.to_string()),
            declared_owner: Some(part_of.library.clone()),
            status: if matches_owner {
                DartPartLinkStatus::Matched
            } else {
                DartPartLinkStatus::DifferentLibrary
            },
            part_span: reference.source_span.clone(),
            part_of_span: Some(part_of.span.clone()),
        });
    }

    DartPartLinkAnalysis { links }
}

fn part_link_without_target(reference: &DartUriReference) -> DartPartLink {
    DartPartLink {
        owner_path: reference.source_path.clone(),
        part_uri: reference.uri.clone(),
        part_path: reference.target_path.clone(),
        declared_owner: None,
        status: if reference.resolution == DartUriResolution::MissingTarget {
            DartPartLinkStatus::MissingTarget
        } else {
            DartPartLinkStatus::UnresolvedTarget
        },
        part_span: reference.source_span.clone(),
        part_of_span: None,
    }
}
