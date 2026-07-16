use serde::Serialize;

use dartscope::{
    DartFileAnalysis, DartGraphqlContractAnalysis, DartProjectAnalysis, DartProjectSummary,
    DartUriGraph, FlutterInventory, JsonContract, to_json_contract_pretty,
};

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

    assert_golden(
        JsonContract::FileAnalysis,
        &file,
        include_str!("fixtures/file-analysis-v1.json"),
    );
    assert_golden(
        JsonContract::ProjectAnalysis,
        &project,
        include_str!("fixtures/project-analysis-v1.json"),
    );
    assert_golden(
        JsonContract::UriGraph,
        &uri_graph,
        include_str!("fixtures/uri-graph-v1.json"),
    );
    assert_golden(
        JsonContract::GraphqlContracts,
        &graphql,
        include_str!("fixtures/graphql-contracts-v1.json"),
    );
    assert_golden(
        JsonContract::FlutterInventory,
        &flutter,
        include_str!("fixtures/flutter-inventory-v1.json"),
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

fn assert_golden<T: Serialize + ?Sized>(contract: JsonContract, value: &T, expected: &str) {
    let actual = to_json_contract_pretty(contract, value).expect("contract must serialize");
    assert_eq!(actual, expected.trim_end());
}
