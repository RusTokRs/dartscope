use dartscope_core::{DartDiagnostic, PubspecInput};

use crate::pubspec::parse_pubspec as parse_conservative_complete;
use crate::pubspec_configuration::parse_pubspec_configuration as parse_conservative;
use crate::pubspec_yaml_marked_analysis::parse_pubspec as parse_marked_complete;
use crate::pubspec_yaml_marked_configuration::parse_pubspec_configuration as parse_marked;

#[test]
fn marked_backend_matches_conservative_configuration_contract() {
    for (case, source) in parity_cases() {
        let conservative = parse_conservative(PubspecInput::new("config\\pubspec.yaml", source));
        let marked = parse_marked(PubspecInput::new("config\\pubspec.yaml", source));

        assert_eq!(
            marked.environment, conservative.environment,
            "environment parity failed for {case}"
        );
        assert_eq!(
            marked.flutter, conservative.flutter,
            "Flutter configuration parity failed for {case}"
        );
        assert_eq!(
            shared_diagnostics(&marked.diagnostics),
            shared_diagnostics(&conservative.diagnostics),
            "diagnostic parity failed for {case}"
        );
    }
}

#[test]
fn marked_backend_matches_conservative_complete_pubspec_contract() {
    for (case, source) in dependency_parity_cases() {
        let conservative =
            parse_conservative_complete(PubspecInput::new("config\\pubspec.yaml", source));
        let marked = parse_marked_complete(PubspecInput::new("config\\pubspec.yaml", source));

        assert_eq!(
            marked.package_name, conservative.package_name,
            "package-name parity failed for {case}"
        );
        assert_eq!(
            marked.dependencies, conservative.dependencies,
            "dependency parity failed for {case}"
        );
        assert_eq!(
            marked.configuration, conservative.configuration,
            "configuration parity failed for {case}"
        );
        assert_eq!(
            shared_diagnostics(&marked.diagnostics),
            shared_diagnostics(&conservative.diagnostics),
            "diagnostic parity failed for {case}"
        );
    }
}

#[test]
fn marked_backend_preserves_crlf_and_unicode_evidence() {
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

    let conservative = parse_conservative(PubspecInput::new("pubspec.yaml", source));
    let marked = parse_marked(PubspecInput::new("pubspec.yaml", source));

    assert_eq!(
        marked.flutter.asset_configurations,
        conservative.flutter.asset_configurations
    );
    assert_eq!(
        marked.flutter.asset_configurations[0].span.byte_start,
        expected_byte_start
    );
    assert_eq!(
        shared_diagnostics(&marked.diagnostics),
        shared_diagnostics(&conservative.diagnostics)
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

fn dependency_parity_cases() -> [(&'static str, &'static str); 4] {
    [
        (
            "nested dependency sources and sections",
            concat!(
                "name: demo\n",
                "dependencies:\n",
                "  flutter:\n",
                "    sdk: flutter\n",
                "  local_package:\n",
                "    path: ../local_package\n",
                "  remote_package:\n",
                "    git:\n",
                "      url: https://example.com/repo.git\n",
                "      ref: stable\n",
                "    version: ^1.0.0\n",
                "  hosted_package:\n",
                "    hosted:\n",
                "      name: hosted_package\n",
                "      url: https://pub.example.com\n",
                "    version: ^2.0.0\n",
                "  workspace_package:\n",
                "    workspace: true\n",
                "dev_dependencies:\n",
                "  test: ^1.25.0\n",
                "dependency_overrides:\n",
                "  local_package:\n",
                "    path: ../override\n",
            ),
        ),
        (
            "inline dependency source mappings",
            concat!(
                "name: demo\n",
                "dependencies:\n",
                "  local_package: { path: ../local_package }\n",
                "  remote_package: { git: { url: \"https://example.com/repo.git?parts=one,two\", ref: stable }, version: ^1.0.0 }\n",
                "  hosted_package: { hosted: { name: hosted_package, url: https://pub.example.com }, version: ^2.0.0 }\n",
                "  workspace_package: { workspace: true }\n",
            ),
        ),
        (
            "wildcard CRLF and Unicode evidence",
            concat!(
                "name: демо\r\n",
                "description: Привет\r\n",
                "dependencies:\r\n",
                "    any_version: *\r\n",
                "    пакет: ^1.2.0\r\n",
            ),
        ),
        (
            "duplicate dependency key",
            concat!(
                "name: demo\n",
                "dependencies:\n",
                "  http: ^1.2.0\n",
                "  http: ^1.3.0\n",
            ),
        ),
    ]
}

fn shared_diagnostics(diagnostics: &[DartDiagnostic]) -> Vec<DartDiagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.code.as_str(),
                "pubspec_duplicate_key"
                    | "pubspec_multiple_documents_unsupported"
                    | "pubspec_invalid_flutter_asset_flavor"
                    | "pubspec_invalid_flutter_asset_platform"
            )
        })
        .cloned()
        .collect()
}
