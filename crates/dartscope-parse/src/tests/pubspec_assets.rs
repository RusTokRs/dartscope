use dartscope_core::PubspecInput;

use crate::parse_pubspec_configuration;

#[test]
fn parses_selectors_and_ordered_transformers() {
    let source = concat!(
        "flutter:\n",
        "  assets:\n",
        "    - path: assets/logo.svg\n",
        "      flavors: [development, production]\n",
        "      platforms:\n",
        "        - android\n",
        "        - ios\n",
        "      transformers:\n",
        "        - package: vector_graphics_compiler\n",
        "          args: ['--tessellate', '--font-size=14']\n",
        "        - package: png_optimizer\n",
    );
    let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

    assert!(analysis.diagnostics.is_empty());
    let asset = &analysis.flutter.asset_configurations[0];
    assert_eq!(asset.flavors.as_slice(), ["development", "production"]);
    assert_eq!(asset.platforms.as_slice(), ["android", "ios"]);
    assert_eq!(asset.transformers.len(), 2);
    assert_eq!(asset.transformers[0].package, "vector_graphics_compiler");
    assert_eq!(
        asset.transformers[0].args.as_slice(),
        ["--tessellate", "--font-size=14"]
    );
    assert_eq!(asset.transformers[1].package, "png_optimizer");
}

#[test]
fn preserves_plain_scalars_containing_colons() {
    let source = concat!(
        "flutter:\n",
        "  assets:\n",
        "    - assets/themes:dark/logo.png\n",
        "    - path: assets/logo.svg\n",
        "      transformers:\n",
        "        - package: vector_graphics_compiler\n",
        "          args: [https://example.com/a:b, '--header: value']\n",
    );
    let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

    assert!(analysis.diagnostics.is_empty());
    assert_eq!(
        analysis.flutter.asset_configurations[0].path,
        "assets/themes:dark/logo.png"
    );
    assert_eq!(
        analysis.flutter.asset_configurations[1].transformers[0]
            .args
            .as_slice(),
        ["https://example.com/a:b", "--header: value"]
    );
}

#[test]
fn rejects_metadata_attached_to_scalar_assets() {
    let source = concat!(
        "flutter:\n",
        "  assets:\n",
        "    - assets/logo.png\n",
        "      flavors: [development]\n",
    );
    let analysis =
        parse_pubspec_configuration(PubspecInput::new("config\\pubspec.yaml", source));

    assert!(analysis.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "pubspec_invalid_flutter_asset"
            && diagnostic.path.as_deref() == Some("config/pubspec.yaml")
    }));
    assert!(analysis.flutter.asset_configurations[0].flavors.is_empty());
}

#[test]
fn rejects_misindented_transformer_fields_and_items() {
    for source in [
        concat!(
            "flutter:\n",
            "  assets:\n",
            "    - path: assets/logo.svg\n",
            "      transformers:\n",
            "        - package: vector_graphics_compiler\n",
            "        args: [bad]\n",
        ),
        concat!(
            "flutter:\n",
            "  assets:\n",
            "    - path: assets/logo.svg\n",
            "      transformers:\n",
            "        - package: first\n",
            "         - package: second\n",
        ),
    ] {
        let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "pubspec_invalid_flutter_asset_transformer"
        }));
    }
}

#[test]
fn accepts_explicitly_empty_asset_lists() {
    let source = concat!(
        "flutter:\n",
        "  assets:\n",
        "    - path: assets/logo.svg\n",
        "      flavors: []\n",
        "      platforms: []\n",
        "      transformers: []\n",
    );
    let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

    assert!(analysis.diagnostics.is_empty());
    let asset = &analysis.flutter.asset_configurations[0];
    assert!(asset.flavors.is_empty());
    assert!(asset.platforms.is_empty());
    assert!(asset.transformers.is_empty());
}
