use dartscope_core::PubspecInput;
use dartscope_parse::{parse_pubspec, parse_pubspec_configuration};

const SOURCE: &str = concat!(
    "name: demo\n",
    "flutter:\n",
    "  default-flavor: production\n",
    "  assets:\n",
    "    - assets/plain.json\n",
    "    - path: assets/logo.svg\n",
    "      flavors:\n",
    "        - development\n",
    "        - production\n",
    "      platforms: [android, ios]\n",
    "      transformers:\n",
    "        - package: vector_graphics_compiler\n",
    "          args: ['--tessellate', '--font-size=14']\n",
    "        - package: png_optimizer\n",
);

#[test]
fn matches_the_structured_asset_configuration_fixture() {
    let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", SOURCE));
    let actual = serde_json::to_value(analysis).expect("serialize pubspec configuration");
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pubspec_asset_configuration.json"))
            .expect("parse structured asset fixture");

    assert_eq!(actual, expected);
}

#[test]
fn complete_pubspec_analysis_embeds_structured_assets() {
    let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", SOURCE));
    assert_eq!(
        analysis.configuration.flutter.default_flavor.as_deref(),
        Some("production")
    );
    assert_eq!(
        analysis.configuration.flutter.asset_selector_policy,
        dartscope_parse::PubspecFlutterAssetSelectorPolicy::V1
    );
    let assets = &analysis.configuration.flutter.asset_configurations;

    assert_eq!(assets.len(), 2);
    assert_eq!(assets[1].path, "assets/logo.svg");
    assert_eq!(assets[1].transformers.len(), 2);
    assert_eq!(
        assets[1].transformers[0].package,
        "vector_graphics_compiler"
    );
    assert_eq!(assets[1].transformers[1].package, "png_optimizer");
}
