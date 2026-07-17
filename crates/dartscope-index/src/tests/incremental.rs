use crate::*;
use dartscope_core::*;
use dartscope_parse::{
    analyze_file, analyze_file_with_references, analyze_project, analyze_project_with_references,
    parse_pubspec,
};

#[test]
fn incremental_snapshots_match_clean_rebuilds_after_update_and_remove() {
    let initial_sources = vec![
        (
            "lib/api.dart",
            "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
        ),
        (
            "lib/client.dart",
            "import 'api.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }\n",
        ),
        (
            "lib/part_owner.dart",
            "part 'src/part.dart';\nclass Owner {}\n",
        ),
        (
            "lib/src/part.dart",
            "part of '../part_owner.dart';\nclass PartThing {}\n",
        ),
    ];
    let initial = reference_project(&initial_sources);
    let mut index = DartWorkspaceIndex::from_reference_project(initial.clone());
    assert_snapshot_matches(&index, &initial, &DartIndexOptions::default());

    let updated_api = analyze_file_with_references(DartFileInput::new(
        "lib/api.dart",
        "const updatedQuery = r'''query Viewer { viewer { id name } }''';\n",
    ));
    let update = index.upsert_file_with_references(updated_api);
    assert_eq!(update.generation, 1);
    assert!(update.rebuilt.project);
    assert!(!update.rebuilt.uri_graph);
    assert!(!update.rebuilt.part_links);
    assert!(update.rebuilt.graphql_contracts);
    assert!(update.rebuilt.identifier_references);

    let updated_sources = vec![
        (
            "lib/api.dart",
            "const updatedQuery = r'''query Viewer { viewer { id name } }''';\n",
        ),
        initial_sources[1],
        initial_sources[2],
        initial_sources[3],
    ];
    let updated = reference_project(&updated_sources);
    assert_snapshot_matches(&index, &updated, &DartIndexOptions::default());

    let removal = index.remove_file("lib\\src\\part.dart");
    assert_eq!(removal.generation, 2);
    assert!(removal.rebuilt.uri_graph);
    assert!(removal.rebuilt.part_links);
    let without_part = reference_project(&updated_sources[..3]);
    assert_snapshot_matches(&index, &without_part, &DartIndexOptions::default());
}

#[test]
fn diagnostic_only_updates_rebuild_only_the_project_projection() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/a.dart", "class A {}\n")],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let retained = index.snapshot();
    let before = index.counters();
    let mut file = retained.project().files[0].clone();
    file.diagnostics.push(
        DartDiagnostic::warning("synthetic", "synthetic diagnostic", None).with_path("lib/a.dart"),
    );

    let update = index.upsert_file(file.clone());
    assert_eq!(
        update.rebuilt,
        DartWorkspaceSubsystems {
            project: true,
            ..DartWorkspaceSubsystems::default()
        }
    );
    assert_eq!(update.changed_paths, vec!["lib/a.dart".to_string()]);
    assert_eq!(update.affected_paths, vec!["lib/a.dart".to_string()]);
    assert_eq!(retained.generation(), 0);
    assert!(retained.project().diagnostics.is_empty());
    assert_eq!(index.snapshot().project().diagnostics.len(), 1);

    let after = index.counters();
    assert_eq!(after.project_rebuilds, before.project_rebuilds + 1);
    assert_eq!(after.uri_graph_rebuilds, before.uri_graph_rebuilds);
    assert_eq!(after.part_link_rebuilds, before.part_link_rebuilds);
    assert_eq!(
        after.namespace_libraries_rebuilt,
        before.namespace_libraries_rebuilt
    );
    assert_eq!(after.graphql_rebuilds, before.graphql_rebuilds);
    assert_eq!(
        after.graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt
    );
    assert_eq!(after.reference_rebuilds, before.reference_rebuilds);

    let no_op = index.upsert_file(file);
    assert!(no_op.is_no_op());
    assert_eq!(no_op.generation, 1);
    assert_eq!(index.counters().no_op_updates, 1);
}

#[test]
fn declaration_changes_report_the_transitive_reverse_dependency_closure() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/a.dart", "import 'b.dart';\nvoid a() { C(); }\n"),
            DartFileInput::new("lib/b.dart", "export 'c.dart';\n"),
            DartFileInput::new("lib/c.dart", "class C {}\n"),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let replacement = analyze_file(DartFileInput::new("lib/c.dart", "class Changed {}\n"));

    let update = index.upsert_file(replacement);

    assert!(update.rebuilt.project);
    assert!(!update.rebuilt.uri_graph);
    assert!(!update.rebuilt.part_links);
    assert!(!update.rebuilt.graphql_contracts);
    assert!(update.rebuilt.identifier_references);
    assert_eq!(
        update.affected_paths,
        vec![
            "lib/a.dart".to_string(),
            "lib/b.dart".to_string(),
            "lib/c.dart".to_string(),
        ]
    );
}

#[test]
fn option_changes_reuse_project_and_part_products() {
    let analysis = reference_project(&[
        (
            "lib/platform.dart",
            "import 'stub.dart' if (dart.library.io) 'io.dart';\n",
        ),
        ("lib/stub.dart", "class PlatformApi {}\n"),
        ("lib/io.dart", "class PlatformApi {}\n"),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );

    let update = index.update_options(options.clone());

    assert_eq!(
        update.rebuilt,
        DartWorkspaceSubsystems {
            project: false,
            uri_graph: true,
            part_links: false,
            graphql_contracts: true,
            identifier_references: true,
        }
    );
    assert_eq!(update.affected_paths.len(), 3);
    assert_eq!(index.snapshot().uri_graph().references.len(), 1);
    assert_eq!(index.snapshot().uri_graph().references[0].uri, "io.dart");
    assert_eq!(index.counters().project_rebuilds, before.project_rebuilds);
    assert_eq!(
        index.counters().part_link_rebuilds,
        before.part_link_rebuilds
    );
    assert_snapshot_matches_project_only(&index, &options);
}

#[test]
fn pubspec_updates_refresh_package_resolution_without_reparsing_files() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "apps/app/lib/client.dart",
                "import 'package:shared/api.dart';\n",
            ),
            DartFileInput::new("packages/shared/lib/api.dart", "class Api {}\n"),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    assert_eq!(
        index.snapshot().uri_graph().references[0].resolution,
        DartUriResolution::UnindexedPackage
    );

    let update = index.upsert_pubspec(parse_pubspec(PubspecInput::new(
        "packages/shared/pubspec.yaml",
        "name: shared\n",
    )));

    assert!(update.rebuilt.uri_graph);
    assert_eq!(update.affected_paths.len(), 2);
    assert_eq!(
        index.snapshot().uri_graph().references[0].resolution,
        DartUriResolution::Resolved
    );
    assert_eq!(
        index.snapshot().uri_graph().references[0]
            .target_path
            .as_deref(),
        Some("packages/shared/lib/api.dart")
    );

    let baseline = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "apps/app/lib/client.dart",
                "import 'package:shared/api.dart';\n",
            ),
            DartFileInput::new("packages/shared/lib/api.dart", "class Api {}\n"),
        ],
        vec![PubspecInput::new(
            "packages/shared/pubspec.yaml",
            "name: shared\n",
        )],
    ));
    assert_eq!(index.snapshot().project(), &baseline);
    assert_eq!(index.snapshot().uri_graph(), &build_uri_graph(&baseline));
}

#[test]
fn workspace_state_and_snapshots_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<DartWorkspaceIndex>();
    assert_send_sync::<DartWorkspaceSnapshot>();
    assert_send_sync::<std::sync::Arc<DartWorkspaceSnapshot>>();
}

#[test]
fn per_file_caches_rebuild_only_relevant_sources() {
    let analysis = reference_project(&[
        ("lib/a.dart", "import 'b.dart';\nvoid useB() { B(); }\n"),
        ("lib/b.dart", "class B {}\n"),
        ("lib/x.dart", "import 'y.dart';\nvoid useY() { Y(); }\n"),
        ("lib/y.dart", "class Y {}\n"),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();

    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/b.dart", "class B { int value = 1; }\n"),
    ));

    assert_eq!(update.affected_paths, vec!["lib/a.dart", "lib/b.dart"]);
    assert_eq!(index.counters().uri_files_rebuilt, before.uri_files_rebuilt);
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
    assert_snapshot_matches(
        &index,
        &reference_project(&[
            ("lib/a.dart", "import 'b.dart';\nvoid useB() { B(); }\n"),
            ("lib/b.dart", "class B { int value = 1; }\n"),
            ("lib/x.dart", "import 'y.dart';\nvoid useY() { Y(); }\n"),
            ("lib/y.dart", "class Y {}\n"),
        ]),
        &DartIndexOptions::default(),
    );
}

#[test]
fn local_reference_fact_changes_do_not_invalidate_importers() {
    let analysis = reference_project(&[
        ("lib/a.dart", "import 'b.dart';\nvoid useB() { B(); }\n"),
        ("lib/b.dart", "class B {}\n"),
        ("lib/root.dart", "import 'a.dart';\n"),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis.clone());
    let mut changed = analyze_file_with_references(DartFileInput::new(
        "lib/a.dart",
        "import 'b.dart';\nvoid useB() { B(); }\n",
    ));
    changed.references[0].name = "Missing".to_string();
    let before = index.counters();

    let update = index.upsert_file_with_references(changed);

    assert_eq!(update.affected_paths, vec!["lib/a.dart"]);
    assert_eq!(index.counters().uri_files_rebuilt, before.uri_files_rebuilt);
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
}

#[test]
fn adding_a_missing_target_rebuilds_only_the_target_and_direct_uri_sources() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/client.dart",
            "import 'target.dart';\n",
        )],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let before = index.counters();
    assert_eq!(
        index.snapshot().uri_graph().references[0].resolution,
        DartUriResolution::MissingTarget
    );

    let update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/target.dart",
        "class Target {}\n",
    )));

    assert_eq!(
        index.counters().uri_files_rebuilt,
        before.uri_files_rebuilt + 2
    );
    assert_eq!(
        index.snapshot().uri_graph().references[0].resolution,
        DartUriResolution::Resolved
    );
    assert_eq!(
        update.affected_paths,
        vec!["lib/client.dart", "lib/target.dart"]
    );
}

#[test]
fn same_name_not_visible_evidence_rebuilds_without_an_import_edge() {
    let analysis = reference_project(&[
        (
            "lib/use.dart",
            "void useHidden() { Hidden(); }
",
        ),
        (
            "lib/hidden.dart",
            "class Hidden {}
",
        ),
        (
            "lib/other.dart",
            "class Other {}
",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    let initial_snapshot = index.snapshot();
    let initial = &initial_snapshot
        .identifier_reference_resolutions()
        .resolutions[0];
    assert_eq!(initial.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(initial.candidates.len(), 1);

    let update =
        index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
            "lib/hidden.dart",
            "class Renamed {}
",
        )));

    assert_eq!(
        update.affected_paths,
        vec!["lib/hidden.dart".to_string(), "lib/use.dart".to_string()]
    );
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/use.dart",
            "void useHidden() { Hidden(); }
",
        ),
        (
            "lib/hidden.dart",
            "class Renamed {}
",
        ),
        (
            "lib/other.dart",
            "class Other {}
",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    let updated_snapshot = index.snapshot();
    let resolution = &updated_snapshot
        .identifier_reference_resolutions()
        .resolutions[0];
    assert_eq!(resolution.status, DartSymbolResolutionStatus::Missing);
    assert!(resolution.candidates.is_empty());
}

#[test]
fn part_membership_changes_rebuild_sibling_reference_sources() {
    let analysis = reference_project(&[
        (
            "lib/owner.dart",
            "part 'left.dart';
part 'right.dart';
class Owner {}
",
        ),
        (
            "lib/left.dart",
            "part of 'owner.dart';
class Shared {}
",
        ),
        (
            "lib/right.dart",
            "part of 'owner.dart';
void useShared() { Shared(); }
",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(analysis);
    let before = index.counters();
    assert_eq!(
        index
            .snapshot()
            .identifier_reference_resolutions()
            .resolutions[0]
            .status,
        DartSymbolResolutionStatus::Resolved
    );

    let update =
        index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
            "lib/left.dart",
            "part of 'different.dart';
class Shared {}
",
        )));

    assert_eq!(
        update.affected_paths,
        vec![
            "lib/left.dart".to_string(),
            "lib/owner.dart".to_string(),
            "lib/right.dart".to_string(),
        ]
    );
    assert_eq!(
        index.counters().reference_files_rebuilt,
        before.reference_files_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/owner.dart",
            "part 'left.dart';
part 'right.dart';
class Owner {}
",
        ),
        (
            "lib/left.dart",
            "part of 'different.dart';
class Shared {}
",
        ),
        (
            "lib/right.dart",
            "part of 'owner.dart';
void useShared() { Shared(); }
",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    assert_eq!(
        index
            .snapshot()
            .identifier_reference_resolutions()
            .resolutions[0]
            .status,
        DartSymbolResolutionStatus::NotVisible
    );
}

#[test]
fn per_library_graphql_cache_rebuilds_only_the_affected_use_library() {
    let initial = reference_project(&[
        (
            "lib/a_api.dart",
            "const viewerQuery = r'''query Viewer { viewer { id } }''';
",
        ),
        (
            "lib/a_client.dart",
            "import 'a_api.dart';
void loadA() { client.query(QueryOptions(document: gql(viewerQuery))); }
",
        ),
        (
            "lib/b_api.dart",
            "const accountQuery = r'''query Account { account { id } }''';
",
        ),
        (
            "lib/b_client.dart",
            "import 'b_api.dart';
void loadB() { client.query(QueryOptions(document: gql(accountQuery))); }
",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    assert_eq!(before.graphql_libraries_rebuilt, 2);

    let update =
        index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
            "lib/a_api.dart",
            "const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';
",
        )));

    assert_eq!(
        update.affected_paths,
        vec!["lib/a_api.dart", "lib/a_client.dart"]
    );
    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/a_api.dart",
            "const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';
",
        ),
        (
            "lib/a_client.dart",
            "import 'a_api.dart';
void loadA() { client.query(QueryOptions(document: gql(viewerQuery))); }
",
        ),
        (
            "lib/b_api.dart",
            "const accountQuery = r'''query Account { account { id } }''';
",
        ),
        (
            "lib/b_client.dart",
            "import 'b_api.dart';
void loadB() { client.query(QueryOptions(document: gql(accountQuery))); }
",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
}

#[test]
fn graphql_not_visible_evidence_rebuilds_without_a_uri_edge() {
    let initial = reference_project(&[
        (
            "lib/use.dart",
            "void load() { client.query(QueryOptions(document: gql(hiddenQuery))); }
",
        ),
        (
            "lib/hidden.dart",
            "const hiddenQuery = r'''query Hidden { hidden { id } }''';
",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    let initial_snapshot = index.snapshot();
    let unresolved = &initial_snapshot.graphql_contracts().unresolved_uses[0];
    assert_eq!(
        unresolved.reason,
        DartGraphqlUnresolvedReason::NotVisibleDeclaration
    );
    assert_eq!(unresolved.candidate_paths, vec!["lib/hidden.dart"]);

    index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
        "lib/hidden.dart",
        "const renamedQuery = r'''query Hidden { hidden { id } }''';
",
    )));

    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/use.dart",
            "void load() { client.query(QueryOptions(document: gql(hiddenQuery))); }
",
        ),
        (
            "lib/hidden.dart",
            "const renamedQuery = r'''query Hidden { hidden { id } }''';
",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    let updated_snapshot = index.snapshot();
    let unresolved = &updated_snapshot.graphql_contracts().unresolved_uses[0];
    assert_eq!(
        unresolved.reason,
        DartGraphqlUnresolvedReason::MissingDeclaration
    );
    assert!(unresolved.candidate_paths.is_empty());
}

#[test]
fn graphql_cache_groups_operation_uses_by_part_library() {
    let initial = reference_project(&[
        (
            "lib/owner.dart",
            "part 'operation.dart';
part 'use.dart';
",
        ),
        (
            "lib/operation.dart",
            "part of 'owner.dart';
const viewerQuery = r'''query Viewer { viewer { id } }''';
",
        ),
        (
            "lib/use.dart",
            "part of 'owner.dart';
void load() { client.query(QueryOptions(document: gql(viewerQuery))); }
",
        ),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(initial);
    let before = index.counters();
    assert_eq!(before.graphql_libraries_rebuilt, 1);
    assert_eq!(index.snapshot().graphql_contracts().bindings.len(), 1);

    index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
        "lib/operation.dart",
        "part of 'owner.dart';
const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';
",
    )));

    assert_eq!(
        index.counters().graphql_libraries_rebuilt,
        before.graphql_libraries_rebuilt + 1
    );
    let baseline = reference_project(&[
        (
            "lib/owner.dart",
            "part 'operation.dart';
part 'use.dart';
",
        ),
        (
            "lib/operation.dart",
            "part of 'owner.dart';
const viewerQuery = r'''query UpdatedViewer { viewer { id name } }''';
",
        ),
        (
            "lib/use.dart",
            "part of 'owner.dart';
void load() { client.query(QueryOptions(document: gql(viewerQuery))); }
",
        ),
    ]);
    assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
}

#[test]
fn deterministic_randomized_update_sequences_match_clean_rebuilds() {
    use std::collections::BTreeMap;

    let mut sources = BTreeMap::from([
        (
            "lib/a.dart".to_string(),
            "import 'b.dart';\nvoid useC() { C(); }\n".to_string(),
        ),
        ("lib/b.dart".to_string(), "export 'c.dart';\n".to_string()),
        ("lib/c.dart".to_string(), "class C {}\n".to_string()),
        ("lib/spare.dart".to_string(), "class Spare {}\n".to_string()),
    ]);
    let initial_pairs: Vec<_> = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    let mut index = DartWorkspaceIndex::from_reference_project(reference_project(&initial_pairs));
    let mut state = 0x5eed_u64;

    for step in 0..64 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        match state % 4 {
            0 => {
                let source = if step % 2 == 0 {
                    "class C { int value = 1; }\n"
                } else {
                    "class C {}\n"
                };
                sources.insert("lib/c.dart".to_string(), source.to_string());
                index.upsert_file_with_references(analyze_file_with_references(
                    DartFileInput::new("lib/c.dart", source),
                ));
            }
            1 => {
                let source = if step % 2 == 0 {
                    "import 'b.dart';\nvoid useC() { C(); C(); }\n"
                } else {
                    "import 'b.dart';\nvoid useC() { C(); }\n"
                };
                sources.insert("lib/a.dart".to_string(), source.to_string());
                index.upsert_file_with_references(analyze_file_with_references(
                    DartFileInput::new("lib/a.dart", source),
                ));
            }
            2 => {
                if sources.remove("lib/c.dart").is_some() {
                    index.remove_file("lib/c.dart");
                } else {
                    let source = "class C {}\n";
                    sources.insert("lib/c.dart".to_string(), source.to_string());
                    index.upsert_file_with_references(analyze_file_with_references(
                        DartFileInput::new("lib/c.dart", source),
                    ));
                }
            }
            _ => {
                let source = if step % 2 == 0 {
                    "export 'c.dart';\nexport 'spare.dart';\n"
                } else {
                    "export 'c.dart';\n"
                };
                sources.insert("lib/b.dart".to_string(), source.to_string());
                index.upsert_file_with_references(analyze_file_with_references(
                    DartFileInput::new("lib/b.dart", source),
                ));
            }
        }

        let pairs: Vec<_> = sources
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect();
        let baseline = reference_project(&pairs);
        assert_snapshot_matches(&index, &baseline, &DartIndexOptions::default());
    }
}

fn reference_project(sources: &[(&str, &str)]) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        sources
            .iter()
            .map(|(path, source)| DartFileInput::new(*path, *source))
            .collect(),
        vec![],
    ))
}

fn assert_snapshot_matches(
    index: &DartWorkspaceIndex,
    baseline: &DartProjectReferenceAnalysis,
    options: &DartIndexOptions,
) {
    let snapshot = index.snapshot();
    assert_eq!(snapshot.project(), &baseline.project);
    assert_eq!(
        snapshot.uri_graph(),
        &build_uri_graph_with_options(&baseline.project, options)
    );
    assert_eq!(
        snapshot.part_links(),
        &analyze_part_links(&baseline.project)
    );
    assert_eq!(
        snapshot.graphql_contracts(),
        &analyze_graphql_contracts_with_options(&baseline.project, options)
    );
    assert_eq!(
        snapshot.identifier_reference_resolutions(),
        &resolve_project_identifier_references_with_options(baseline, options)
    );
}

fn assert_snapshot_matches_project_only(index: &DartWorkspaceIndex, options: &DartIndexOptions) {
    let snapshot = index.snapshot();
    assert_eq!(
        snapshot.uri_graph(),
        &build_uri_graph_with_options(snapshot.project(), options)
    );
    assert_eq!(
        snapshot.part_links(),
        &analyze_part_links(snapshot.project())
    );
    assert_eq!(
        snapshot.graphql_contracts(),
        &analyze_graphql_contracts_with_options(snapshot.project(), options)
    );
}
