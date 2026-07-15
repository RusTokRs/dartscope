use dartscope_core::{DartDiagnostic, PubspecInput};
use dartscope_parse::{parse_pubspec, parse_pubspec_configuration};

#[test]
fn focused_and_complete_apis_share_configuration_contract() {
    for (case, source) in parity_cases() {
        let focused =
            parse_pubspec_configuration(PubspecInput::new("config\\pubspec.yaml", source));
        let complete = parse_pubspec(PubspecInput::new("config\\pubspec.yaml", source));

        assert_eq!(
            complete.configuration.environment, focused.environment,
            "environment parity failed for {case}"
        );
        assert_eq!(
            complete.configuration.flutter, focused.flutter,
            "Flutter configuration parity failed for {case}"
        );
        assert_eq!(
            shared_diagnostics(&complete.diagnostics),
            shared_diagnostics(&focused.diagnostics),
            "diagnostic parity failed for {case}"
        );
    }
}

#[test]
fn document_preparation_preserves_crlf_and_unicode_evidence() {
    let source = concat!(
        "---\r\n",
        "name: demo\r\n",
        "description: Привет\r\n",
        "flutter:\r\n",
        "  assets:\r\n",
        "    - path: assets/иконка.png\r\n",
        "      platforms: [android, web]\r\n",
        "...\r\n",
    );
    let expected_byte_start = source
        .find("    - path: assets/иконка.png")
        .expect("asset declaration must exist");

    let focused = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));
    let complete = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

    assert!(focused.diagnostics.is_empty());
    assert!(complete.diagnostics.is_empty());
    assert_eq!(
        focused.flutter.asset_configurations[0].span.byte_start,
        expected_byte_start
    );
    assert_eq!(
        complete.configuration.flutter.asset_configurations[0]
            .span
            .byte_start,
        expected_byte_start
    );
}

fn parity_cases() -> [(&'static str, &'static str); 4] {
    [
        (
            "valid structured pubspec",
            concat!(
                "name: demo\n",
                "environment:\n",
                "  sdk: ^3.4.0\n",
                "dependencies:\n",
                "  flutter:\n",
                "    sdk: flutter\n",
                "flutter:\n",
                "  generate: true\n",
                "  assets:\n",
                "    - path: assets/logo.svg\n",
                "      flavors: [development, customer-a]\n",
                "      platforms: [android, ios, web]\n",
                "      transformers:\n",
                "        - package: vector_graphics_compiler\n",
                "          args: ['--tessellate']\n",
            ),
        ),
        (
            "duplicate direct mapping key",
            concat!(
                "name: demo\n",
                "flutter:\n",
                "  generate: true\n",
                "  generate: false\n",
            ),
        ),
        (
            "additional YAML document",
            concat!(
                "name: demo\n",
                "flutter:\n",
                "  generate: true\n",
                "---\n",
                "name: ignored\n",
                "flutter:\n",
                "  generate: false\n",
            ),
        ),
        (
            "invalid asset selectors",
            concat!(
                "name: demo\n",
                "flutter:\n",
                "  assets:\n",
                "    - path: assets/fuchsia.bin\n",
                "      flavors: ['']\n",
                "      platforms: [fuchsia]\n",
            ),
        ),
    ]
}

fn shared_diagnostics(diagnostics: &[DartDiagnostic]) -> Vec<serde_json::Value> {
    diagnostics
        .iter()
        .filter(|diagnostic| is_shared_diagnostic(&diagnostic.code))
        .map(|diagnostic| serde_json::to_value(diagnostic).expect("serialize diagnostic"))
        .collect()
}

fn is_shared_diagnostic(code: &str) -> bool {
    matches!(
        code,
        "pubspec_duplicate_key"
            | "pubspec_multiple_documents_unsupported"
            | "pubspec_invalid_flutter_asset_flavor"
            | "pubspec_invalid_flutter_asset_platform"
    )
}
