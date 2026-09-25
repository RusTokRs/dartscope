use dartscope::{
    DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartFileLanguage, DartGraphqlContractAnalysis,
    DartImport, DartLintAnalysis, DartProjectAnalysis, DartProjectSummary, DartUriGraph,
    FlutterFileHints, FlutterInventory, JsonContract, SourceSpan, to_json_contract_pretty,
};

macro_rules! assert_golden {
    ($contract:expr, $value:expr, $expected:expr) => {{
        let actual = to_json_contract_pretty($contract, $value).expect("contract must serialize");
        assert_eq!(actual, $expected.trim_end());
    }};
}

#[test]
fn checked_in_v1_golden_contracts_match_public_models() {
    let file = DartFileAnalysis::empty("lib/main.dart");
    let project = DartProjectAnalysis {
        root: ".".to_string(),
        files: Vec::new(),
        pubspecs: Vec::new(),
        package_configs: Vec::new(),
        summary: DartProjectSummary::default(),
        diagnostics: Vec::new(),
    };
    let uri_graph = DartUriGraph::default();
    let graphql = DartGraphqlContractAnalysis::default();
    let flutter = FlutterInventory::default();
    let lint = DartLintAnalysis::default();

    assert_golden!(
        JsonContract::FileAnalysis,
        &file,
        include_str!("fixtures/file-analysis-v1.json")
    );
    assert_golden!(
        JsonContract::ProjectAnalysis,
        &project,
        include_str!("fixtures/project-analysis-v1.json")
    );
    assert_golden!(
        JsonContract::UriGraph,
        &uri_graph,
        include_str!("fixtures/uri-graph-v1.json")
    );
    assert_golden!(
        JsonContract::GraphqlContracts,
        &graphql,
        include_str!("fixtures/graphql-contracts-v1.json")
    );
    assert_golden!(
        JsonContract::FlutterInventory,
        &flutter,
        include_str!("fixtures/flutter-inventory-v1.json")
    );
    assert_golden!(
        JsonContract::LintAnalysis,
        &lint,
        include_str!("fixtures/lint-analysis-v1.json")
    );
}

#[test]
fn populated_v1_golden_contracts_match_public_models() {
    // Populated file with 1 import, 1 class declaration — catches regressions inside entry objects
    let file = DartFileAnalysis {
        path: "lib/main.dart".to_string(),
        language: DartFileLanguage::Dart,
        library: None,
        imports: vec![DartImport {
            uri: "package:foo/bar.dart".to_string(),
            configurations: Vec::new(),
            is_deferred: false,
            prefix: None,
            combinators: Vec::new(),
            span: SourceSpan {
                byte_start: 0,
                byte_end: 27,
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 28,
            },
        }],
        exports: Vec::new(),
        parts: Vec::new(),
        part_of: None,
        declarations: vec![DartDeclaration {
            name: "MyClass".to_string(),
            kind: DartDeclarationKind::Class,
            span: SourceSpan {
                byte_start: 0,
                byte_end: 15,
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 16,
            },
            extends: None,
            mixes_in: Vec::new(),
            symbol_id: Some("lib/main.dart::class:MyClass".to_string()),
            parent_symbol_id: None,
            declaration_span: Some(SourceSpan {
                byte_start: 0,
                byte_end: 15,
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 16,
            }),
        }],
        string_constants: Vec::new(),
        graphql_operations: Vec::new(),
        graphql_operation_uses: Vec::new(),
        invocations: Vec::new(),
        flutter: FlutterFileHints::default(),
        diagnostics: Vec::new(),
    };
    let project = DartProjectAnalysis {
        root: ".".to_string(),
        files: vec![file.clone()],
        pubspecs: Vec::new(),
        package_configs: Vec::new(),
        summary: DartProjectSummary {
            dart_files: 1,
            pubspecs: 0,
            package_configs: 0,
            imports: 1,
            exports: 0,
            parts: 0,
            declarations: 1,
            string_constants: 0,
            graphql_operations: 0,
            graphql_operation_uses: 0,
            flutter_widgets: 0,
            flutter_routes: 0,
            flutter_assets: 0,
            flutter_localizations: 0,
            package_dependencies: 0,
            diagnostics: 0,
        },
        diagnostics: Vec::new(),
    };

    assert_golden!(
        JsonContract::FileAnalysis,
        &file,
        include_str!("fixtures/file-analysis-populated-v1.json")
    );
    assert_golden!(
        JsonContract::ProjectAnalysis,
        &project,
        include_str!("fixtures/project-analysis-populated-v1.json")
    );
}

#[test]
fn every_registered_contract_is_listed_in_the_compatibility_policy() {
    let policy = include_str!("../../../docs/development/json-contracts.md");

    for contract in JsonContract::ALL {
        let marker = format!("`{}` v{}", contract.schema(), contract.version());
        assert!(
            policy.contains(&marker),
            "missing compatibility or migration entry for {marker}"
        );
    }
}
