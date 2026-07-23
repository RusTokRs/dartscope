use crate::*;
use dartscope_core::*;
use dartscope_parse::analyze_project;

fn analyzed_project(
    mut files: Vec<DartFileInput>,
    mut pubspecs: Vec<PubspecInput>,
    mut package_configs: Vec<PackageConfigInput>,
    reverse: bool,
) -> DartProjectAnalysis {
    if reverse {
        files.reverse();
        pubspecs.reverse();
        package_configs.reverse();
    }

    analyze_project(
        DartProjectInput::new(".", files, pubspecs).with_package_configs(package_configs),
    )
}

#[test]
fn uri_graph_is_stable_across_file_and_pubspec_input_order() {
    let files = vec![
        DartFileInput::new(
            "lib/client.dart",
            concat!(
                "import 'src/local.dart';\n",
                "import 'package:duplicate/api.dart';\n",
                "import 'src/stub.dart' if (dart.library.io) 'src/io.dart';\n",
            ),
        ),
        DartFileInput::new("lib/src/local.dart", "class Local {}\n"),
        DartFileInput::new("lib/src/stub.dart", "class Platform {}\n"),
        DartFileInput::new("lib/src/io.dart", "class Platform {}\n"),
        DartFileInput::new("one/lib/api.dart", "class Api {}\n"),
        DartFileInput::new("two/lib/api.dart", "class Api {}\n"),
    ];
    let pubspecs = vec![
        PubspecInput::new("one/pubspec.yaml", "name: duplicate\n"),
        PubspecInput::new("two/pubspec.yaml", "name: duplicate\n"),
    ];

    let forward = analyzed_project(files.clone(), pubspecs.clone(), Vec::new(), false);
    let reversed = analyzed_project(files, pubspecs, Vec::new(), true);
    let forward_graph = build_uri_graph(&forward);
    let reversed_graph = build_uri_graph(&reversed);

    assert_eq!(forward_graph, reversed_graph);
    let ambiguous = forward_graph
        .references
        .iter()
        .find(|reference| reference.uri == "package:duplicate/api.dart")
        .expect("ambiguous package reference");
    assert_eq!(ambiguous.resolution, DartUriResolution::AmbiguousPackage);
    assert_eq!(
        ambiguous.candidate_paths,
        ["one/lib/api.dart", "two/lib/api.dart"]
    );
}

#[test]
fn nearest_package_config_is_stable_across_config_input_order() {
    let files = vec![
        DartFileInput::new(
            "apps/demo/lib/client.dart",
            "import 'package:shared/api.dart';\n",
        ),
        DartFileInput::new("packages/shared/lib/api.dart", "class SharedApi {}\n"),
        DartFileInput::new("packages/wrong/lib/api.dart", "class WrongApi {}\n"),
    ];
    let package_configs = vec![
        PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"shared","rootUri":"../packages/wrong/","packageUri":"lib/"}
  ]
}"#,
        ),
        PackageConfigInput::new(
            "apps/demo/.dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"shared","rootUri":"../../../packages/shared/","packageUri":"lib/"}
  ]
}"#,
        ),
    ];

    let forward = analyzed_project(files.clone(), Vec::new(), package_configs.clone(), false);
    let reversed = analyzed_project(files, Vec::new(), package_configs, true);
    let forward_graph = build_uri_graph(&forward);
    let reversed_graph = build_uri_graph(&reversed);

    assert_eq!(forward_graph, reversed_graph);
    assert_eq!(forward_graph.references.len(), 1);
    assert_eq!(
        forward_graph.references[0].target_path.as_deref(),
        Some("packages/shared/lib/api.dart")
    );
    assert_eq!(
        forward_graph.references[0].resolution,
        DartUriResolution::Resolved
    );
}

#[test]
fn graphql_contracts_are_stable_across_file_input_order() {
    let files = vec![
        DartFileInput::new(
            "lib/documents.dart",
            "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
        ),
        DartFileInput::new(
            "lib/api.dart",
            "export 'documents.dart' show viewerQuery;\n",
        ),
        DartFileInput::new(
            "lib/a.dart",
            "const duplicateQuery = r'''query DuplicateA { viewer { id } }''';\n",
        ),
        DartFileInput::new(
            "lib/b.dart",
            "const duplicateQuery = r'''query DuplicateB { viewer { id } }''';\n",
        ),
        DartFileInput::new(
            "lib/client.dart",
            concat!(
                "import 'api.dart';\n",
                "import 'a.dart';\n",
                "import 'b.dart';\n",
                "void load() {\n",
                "  client.query(QueryOptions(document: gql(viewerQuery)));\n",
                "  client.query(QueryOptions(document: gql(duplicateQuery)));\n",
                "}\n",
            ),
        ),
    ];

    let forward = analyzed_project(files.clone(), Vec::new(), Vec::new(), false);
    let reversed = analyzed_project(files, Vec::new(), Vec::new(), true);
    let forward_contracts = analyze_graphql_contracts(&forward);
    let reversed_contracts = analyze_graphql_contracts(&reversed);

    assert_eq!(forward_contracts, reversed_contracts);
    assert_eq!(forward_contracts.bindings.len(), 1);
    assert_eq!(forward_contracts.unresolved_uses.len(), 1);
    assert_eq!(
        forward_contracts.bindings[0].resolution_basis,
        DartGraphqlBindingResolution::ReExport
    );
    assert_eq!(
        forward_contracts.unresolved_uses[0].reason,
        DartGraphqlUnresolvedReason::AmbiguousDeclaration
    );
    assert_eq!(
        forward_contracts.unresolved_uses[0].candidate_paths,
        ["lib/a.dart", "lib/b.dart"]
    );
}
