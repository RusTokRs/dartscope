use crate::*;
use dartscope_core::*;
use dartscope_parse::analyze_project;

#[test]
fn resolves_relative_package_sdk_and_missing_uri_references() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "packages/app/lib/app.dart",
                r#"
import 'dart:async';
import 'src/local.dart';
import 'package:shared/shared.dart';
export 'src/missing.dart';
part 'src/app_part.dart';
"#,
            ),
            DartFileInput::new("packages/app/lib/src/local.dart", "class Local {}"),
            DartFileInput::new(
                "packages/app/lib/src/app_part.dart",
                "part of '../app.dart';",
            ),
            DartFileInput::new("packages/shared/lib/shared.dart", "class Shared {}"),
        ],
        vec![
            dartscope_core::PubspecInput::new("packages/app/pubspec.yaml", "name: app\n"),
            dartscope_core::PubspecInput::new("packages/shared/pubspec.yaml", "name: shared\n"),
        ],
    ));

    let graph = build_uri_graph(&project);

    assert_eq!(graph.references.len(), 5);
    assert_eq!(graph.references[0].resolution, DartUriResolution::External);
    assert_eq!(
        graph.references[1].target_path.as_deref(),
        Some("packages/app/lib/src/local.dart")
    );
    assert_eq!(graph.references[1].resolution, DartUriResolution::Resolved);
    assert_eq!(
        graph.references[2].target_path.as_deref(),
        Some("packages/shared/lib/shared.dart")
    );
    assert_eq!(graph.references[2].resolution, DartUriResolution::Resolved);
    assert_eq!(
        graph.references[3].resolution,
        DartUriResolution::MissingTarget
    );
    assert_eq!(graph.references[4].kind, DartUriReferenceKind::Part);
    assert_eq!(graph.references[4].resolution, DartUriResolution::Resolved);
}

#[test]
fn uri_graph_json_schema_fixture_is_stable() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/client.dart", "import 'docs.dart' show viewerQuery;\n"),
            DartFileInput::new("lib/docs.dart", "const viewerQuery = 'query Viewer';\n"),
        ],
        vec![],
    ));

    let graph = build_uri_graph(&project);

    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::json!({
            "references": [
                {
                    "source_path": "lib/client.dart",
                    "source_span": {
                        "byte_start": 0,
                        "byte_end": 36,
                        "start_line": 1,
                        "start_column": 1,
                        "end_line": 1,
                        "end_column": 37
                    },
                    "uri": "docs.dart",
                    "condition": null,
                    "kind": "import",
                    "resolution": "resolved",
                    "target_path": "lib/docs.dart",
                    "target_uri": null,
                    "candidate_paths": []
                }
            ]
        })
    );
}

#[test]
fn part_links_json_schema_fixture_is_stable() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/models.dart", "part 'src/model.dart';\n"),
            DartFileInput::new(
                "lib/src/model.dart",
                "part of '../models.dart';\nclass Model {}\n",
            ),
        ],
        vec![],
    ));

    let analysis = analyze_part_links(&project);

    assert_eq!(
        serde_json::to_value(&analysis).unwrap(),
        serde_json::json!({
            "links": [
                {
                    "owner_path": "lib/models.dart",
                    "part_uri": "src/model.dart",
                    "part_path": "lib/src/model.dart",
                    "declared_owner": "../models.dart",
                    "status": "matched",
                    "part_span": {
                        "byte_start": 0,
                        "byte_end": 22,
                        "start_line": 1,
                        "start_column": 1,
                        "end_line": 1,
                        "end_column": 23
                    },
                    "part_of_span": {
                        "byte_start": 0,
                        "byte_end": 25,
                        "start_line": 1,
                        "start_column": 1,
                        "end_line": 1,
                        "end_column": 26
                    }
                }
            ]
        })
    );
}

#[test]
fn graphql_contract_json_schema_fixture_is_stable() {
    let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/client.dart",
                "const viewerQuery = r'''\nquery Viewer($id: ID!) { viewer(id: $id) { id } }\n''';\n\nvoid load() {\n  client.query(QueryOptions(document: gql(viewerQuery), variables: {'id': id, 'extra': true}));\n}\n",
            )],
            vec![],
        ));

    let analysis = analyze_graphql_contracts(&project);

    assert_eq!(
        serde_json::to_value(&analysis).unwrap(),
        serde_json::json!({
            "bindings": [
                {
                    "constant_name": "viewerQuery",
                    "resolution_basis": "same_file",
                    "operation_name": "Viewer",
                    "operation_type": "query",
                    "client_call": "query",
                    "call_compatibility": "match",
                    "declared_variable_names": ["id"],
                    "supplied_variable_names": ["extra", "id"],
                    "missing_variable_names": [],
                    "unexpected_variable_names": ["extra"],
                    "variable_compatibility": "mismatch",
                    "operation_path": "lib/client.dart",
                    "operation_span": {
                        "byte_start": 0,
                        "byte_end": 24,
                        "start_line": 1,
                        "start_column": 1,
                        "end_line": 1,
                        "end_column": 25
                    },
                    "use_path": "lib/client.dart",
                    "use_span": {
                        "byte_start": 95,
                        "byte_end": 190,
                        "start_line": 6,
                        "start_column": 1,
                        "end_line": 6,
                        "end_column": 96
                    },
                    "enclosing_symbol": {
                        "name": "load",
                        "kind": "callable"
                    }
                }
            ],
            "unresolved_uses": []
        })
    );
}

#[test]
fn reports_unknown_and_ambiguous_packages() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/use.dart",
            "import 'package:missing/api.dart';\nimport 'package:duplicate/api.dart';",
        )],
        vec![
            dartscope_core::PubspecInput::new("one/pubspec.yaml", "name: duplicate\n"),
            dartscope_core::PubspecInput::new("two/pubspec.yaml", "name: duplicate\n"),
        ],
    ));

    let graph = build_uri_graph(&project);

    assert_eq!(
        graph.references[0].resolution,
        DartUriResolution::UnindexedPackage
    );
    assert_eq!(
        graph.references[1].resolution,
        DartUriResolution::AmbiguousPackage
    );
    assert_eq!(
        graph.references[1].candidate_paths,
        ["one/lib/api.dart", "two/lib/api.dart"]
    );
}

#[test]
fn resolves_every_conditional_uri_without_selecting_an_environment() {
    let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/platform.dart",
                    "import 'src/stub.dart' if (dart.library.io) 'src/io.dart' if (dart.library.js_interop) 'src/web.dart';\n",
                ),
                DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/io.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/web.dart", "class PlatformApi {}\n"),
            ],
            vec![],
        ));

    let graph = build_uri_graph(&project);

    assert_eq!(graph.references.len(), 3);
    assert!(graph
        .references
        .iter()
        .all(|reference| reference.resolution == DartUriResolution::Resolved));
    assert!(graph
        .references
        .iter()
        .any(|reference| { reference.uri == "src/stub.dart" && reference.condition.is_none() }));
    assert!(graph.references.iter().any(|reference| {
        reference.uri == "src/io.dart" && reference.condition.as_deref() == Some("dart.library.io")
    }));
    assert!(graph.references.iter().any(|reference| {
        reference.uri == "src/web.dart"
            && reference.condition.as_deref() == Some("dart.library.js_interop")
    }));
}

#[test]
fn selects_the_first_matching_conditional_uri_when_environment_is_explicit() {
    let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/platform.dart",
                    "import 'src/stub.dart' if (flavor == 'prod') 'src/prod.dart' if (flavor == 'dev') 'src/dev.dart';\n",
                ),
                DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/prod.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/dev.dart", "class PlatformApi {}\n"),
            ],
            vec![],
        ));
    let options = DartIndexOptions::default()
        .with_compilation_environment(DartCompilationEnvironment::from_pairs([("flavor", "prod")]));

    let graph = build_uri_graph_with_options(&project, &options);

    assert_eq!(graph.references.len(), 1);
    assert_eq!(graph.references[0].uri, "src/prod.dart");
    assert_eq!(
        graph.references[0].condition.as_deref(),
        Some("flavor == 'prod'")
    );
    assert_eq!(graph.references[0].resolution, DartUriResolution::Resolved);
}

#[test]
fn falls_back_to_default_conditional_uri_when_environment_does_not_match() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/platform.dart",
                "import 'src/stub.dart' if (dart.library.io) 'src/io.dart';\n",
            ),
            DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
            DartFileInput::new("lib/src/io.dart", "class PlatformApi {}\n"),
        ],
        vec![],
    ));
    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "false")]),
    );

    let graph = build_uri_graph_with_options(&project, &options);

    assert_eq!(graph.references.len(), 1);
    assert_eq!(graph.references[0].uri, "src/stub.dart");
    assert_eq!(graph.references[0].condition, None);
    assert_eq!(graph.references[0].resolution, DartUriResolution::Resolved);
}

#[test]
fn resolves_package_uris_through_the_nearest_package_config() {
    let project = analyze_project(
        DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "apps/demo/lib/client.dart",
                    "import 'package:shared/api.dart';\nimport 'package:graphql/client.dart';\n",
                ),
                DartFileInput::new("packages/shared/lib/api.dart", "class Api {}\n"),
            ],
            vec![],
        )
        .with_package_configs(vec![PackageConfigInput::new(
            "apps/demo/.dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"shared","rootUri":"../../../packages/shared/","packageUri":"lib/"},
    {"name":"graphql","rootUri":"file:///cache/graphql-5.2.0/","packageUri":"lib/"}
  ]
}"#,
        )]),
    );

    let graph = build_uri_graph(&project);
    let shared = graph
        .references
        .iter()
        .find(|reference| reference.uri == "package:shared/api.dart")
        .unwrap();
    assert_eq!(shared.resolution, DartUriResolution::Resolved);
    assert_eq!(
        shared.target_path.as_deref(),
        Some("packages/shared/lib/api.dart")
    );
    assert_eq!(
        shared.target_uri.as_deref(),
        Some("file:///__dartscope_project__/packages/shared/lib/api.dart")
    );

    let graphql = graph
        .references
        .iter()
        .find(|reference| reference.uri == "package:graphql/client.dart")
        .unwrap();
    assert_eq!(graphql.resolution, DartUriResolution::ResolvedExternal);
    assert_eq!(graphql.target_path, None);
    assert_eq!(
        graphql.target_uri.as_deref(),
        Some("file:///cache/graphql-5.2.0/lib/client.dart")
    );
}

#[test]
fn a_nested_package_config_overrides_an_ancestor_config() {
    let project = analyze_project(
            DartProjectInput::new(
                ".",
                vec![DartFileInput::new(
                    "apps/demo/lib/client.dart",
                    "import 'package:shared/api.dart';\n",
                )],
                vec![],
            )
            .with_package_configs(vec![
                PackageConfigInput::new(
                    ".dart_tool/package_config.json",
                    r#"{"configVersion":2,"packages":[{"name":"shared","rootUri":"../packages/shared/","packageUri":"lib/"}]}"#,
                ),
                PackageConfigInput::new(
                    "apps/demo/.dart_tool/package_config.json",
                    r#"{"configVersion":2,"packages":[{"name":"shared","rootUri":"file:///nested/shared/","packageUri":"lib/"}]}"#,
                ),
            ]),
        );

    let graph = build_uri_graph(&project);

    assert_eq!(graph.references.len(), 1);
    assert_eq!(
        graph.references[0].resolution,
        DartUriResolution::ResolvedExternal
    );
    assert_eq!(
        graph.references[0].target_uri.as_deref(),
        Some("file:///nested/shared/lib/api.dart")
    );
}

#[test]
fn does_not_fall_back_when_the_nearest_package_config_is_invalid() {
    let project = analyze_project(
        DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "apps/demo/lib/client.dart",
                "import 'package:shared/api.dart';\n",
            )],
            vec![],
        )
        .with_package_configs(vec![PackageConfigInput::new(
            "apps/demo/.dart_tool/package_config.json",
            r#"{"configVersion":3,"packages":[]}"#,
        )]),
    );

    let graph = build_uri_graph(&project);

    assert_eq!(
        graph.references[0].resolution,
        DartUriResolution::InvalidConfiguration
    );
}

#[test]
fn requires_an_environment_before_resolving_a_conditional_namespace() {
    let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/stub.dart",
                    "const viewerQuery = r'''query StubViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/io.dart",
                    "const viewerQuery = r'''query IoViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'stub.dart' if (dart.library.io) 'io.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

    let analysis = analyze_graphql_contracts(&project);

    assert!(analysis.bindings.is_empty());
    assert_eq!(analysis.unresolved_uses.len(), 1);
    assert_eq!(
        analysis.unresolved_uses[0].reason,
        DartGraphqlUnresolvedReason::ConditionalEnvironmentRequired
    );
}

#[test]
fn resolves_a_conditional_namespace_when_environment_is_explicit() {
    let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/stub.dart",
                    "const viewerQuery = r'''query StubViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/io.dart",
                    "const viewerQuery = r'''query IoViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'stub.dart' if (dart.library.io) 'io.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));
    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );

    let analysis = analyze_graphql_contracts_with_options(&project, &options);

    assert!(analysis.unresolved_uses.is_empty());
    assert_eq!(analysis.bindings.len(), 1);
    assert_eq!(analysis.bindings[0].operation_path, "lib/io.dart");
    assert_eq!(
        analysis.bindings[0].resolution_basis,
        DartGraphqlBindingResolution::DirectImport
    );
}
