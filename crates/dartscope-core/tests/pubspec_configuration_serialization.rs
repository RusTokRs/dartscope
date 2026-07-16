use dartscope_core::SourceSpan;
use dartscope_core::pubspec::{
    PubspecConfigurationAnalysis, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetSelectorPolicy, PubspecFlutterConfiguration, PubspecFlutterFont,
    PubspecFlutterFontFamily,
};

#[test]
fn serializes_the_core_pubspec_configuration_shape() {
    let analysis = PubspecConfigurationAnalysis {
        path: "pubspec.yaml".to_string(),
        environment: vec![PubspecEnvironmentConstraint {
            name: "sdk".to_string(),
            constraint: ">=3.4.0 <4.0.0".to_string(),
            span: span(20, 23, 3, 3, 6),
        }],
        flutter: PubspecFlutterConfiguration {
            uses_material_design: Some(true),
            generate_localizations: Some(true),
            default_flavor: Some("production".to_string()),
            asset_selector_policy: PubspecFlutterAssetSelectorPolicy::V1,
            assets: vec![PubspecFlutterAsset {
                path: "assets/images/".to_string(),
                span: span(80, 98, 7, 1, 19),
            }],
            asset_configurations: Vec::new(),
            fonts: vec![PubspecFlutterFontFamily {
                family: "Inter".to_string(),
                fonts: vec![PubspecFlutterFont {
                    asset: "fonts/Inter-Bold.ttf".to_string(),
                    style: Some("normal".to_string()),
                    weight: Some(700),
                    span: span(120, 153, 11, 1, 34),
                }],
                span: span(100, 119, 9, 1, 20),
            }],
        },
        diagnostics: Vec::new(),
    };

    let actual = serde_json::to_value(&analysis).expect("serialize pubspec configuration");
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pubspec_configuration.json"))
            .expect("parse pubspec configuration fixture");

    assert_eq!(actual, expected);
    let round_trip: PubspecConfigurationAnalysis =
        serde_json::from_value(expected).expect("deserialize pubspec configuration");
    assert_eq!(round_trip, analysis);
}

fn span(
    byte_start: usize,
    byte_end: usize,
    line: usize,
    start_column: usize,
    end_column: usize,
) -> SourceSpan {
    SourceSpan {
        byte_start,
        byte_end,
        start_line: line,
        start_column,
        end_line: line,
        end_column,
    }
}

#[test]
fn defaults_selector_policy_and_default_flavor_for_legacy_payloads() {
    let legacy = serde_json::json!({
        "path": "pubspec.yaml",
        "environment": [],
        "flutter": {
            "assets": [],
            "fonts": []
        },
        "diagnostics": []
    });

    let analysis: PubspecConfigurationAnalysis =
        serde_json::from_value(legacy).expect("deserialize legacy pubspec configuration");

    assert_eq!(analysis.flutter.default_flavor, None);
    assert_eq!(
        analysis.flutter.asset_selector_policy,
        PubspecFlutterAssetSelectorPolicy::V1
    );
}
