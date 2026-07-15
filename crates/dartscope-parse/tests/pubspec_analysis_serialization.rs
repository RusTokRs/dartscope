use dartscope_core::{DartProjectInput, PubspecAnalysis, PubspecInput};
use dartscope_parse::{analyze_project, parse_pubspec};

const SOURCE: &str = concat!(
    "name: demo\n",
    "environment:\n",
    "  sdk: ^3.4.0\n",
    "dependencies:\n",
    "  flutter:\n",
    "    sdk: flutter\n",
    "flutter:\n",
    "  generate: true\n",
    "  assets:\n",
    "    - assets/\n",
);

#[test]
fn matches_the_complete_pubspec_analysis_fixture() {
    let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", SOURCE));
    let actual = serde_json::to_value(analysis).expect("serialize pubspec analysis");
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/pubspec_analysis.json"
    ))
    .expect("parse pubspec analysis fixture");

    assert_eq!(actual, expected);
}

#[test]
fn defaults_configuration_when_deserializing_legacy_json() {
    let analysis: PubspecAnalysis = serde_json::from_value(serde_json::json!({
        "path": "pubspec.yaml",
        "package_name": "demo",
        "dependencies": [],
        "diagnostics": []
    }))
    .expect("deserialize legacy pubspec analysis");

    assert!(analysis.configuration.environment.is_empty());
    assert!(analysis.configuration.flutter.assets.is_empty());
    assert!(analysis.configuration.flutter.fonts.is_empty());
}

#[test]
fn project_analysis_uses_the_complete_pubspec_parser() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        Vec::new(),
        vec![PubspecInput::new("pubspec.yaml", SOURCE)],
    ));

    assert_eq!(project.pubspecs.len(), 1);
    assert_eq!(project.pubspecs[0].configuration.environment.len(), 1);
    assert_eq!(
        project.pubspecs[0]
            .configuration
            .flutter
            .generate_localizations,
        Some(true)
    );
}
