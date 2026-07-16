use dartscope_core::{PubspecDependencySection, PubspecInput};

use crate::{parse_pubspec, parse_pubspec_configuration};

#[test]
fn preserves_structured_configuration_contract() {
    let source = concat!(
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
    );

    let analysis = parse_pubspec_configuration(PubspecInput::new("config\\pubspec.yaml", source));

    assert_eq!(analysis.path, "config/pubspec.yaml");
    assert_eq!(analysis.environment.len(), 1);
    assert_eq!(analysis.environment[0].name, "sdk");
    assert_eq!(analysis.environment[0].constraint, "^3.4.0");
    assert_eq!(analysis.flutter.generate_localizations, Some(true));
    assert_eq!(analysis.flutter.asset_configurations.len(), 1);
    let asset = &analysis.flutter.asset_configurations[0];
    assert_eq!(asset.path, "assets/logo.svg");
    assert_eq!(asset.flavors, ["development", "customer-a"]);
    assert_eq!(asset.platforms, ["android", "ios", "web"]);
    assert_eq!(asset.transformers.len(), 1);
    assert_eq!(asset.transformers[0].package, "vector_graphics_compiler");
    assert_eq!(asset.transformers[0].args, ["--tessellate"]);
    assert!(analysis.diagnostics.is_empty());
}

#[test]
fn preserves_configuration_diagnostic_contract() {
    let duplicate = parse_pubspec_configuration(PubspecInput::new(
        "config\\pubspec.yaml",
        concat!(
            "name: demo\n",
            "flutter:\n",
            "  generate: true\n",
            "  generate: false\n",
        ),
    ));
    assert_eq!(duplicate.flutter.generate_localizations, Some(false));
    assert!(has_diagnostic_at(
        &duplicate.diagnostics,
        "pubspec_duplicate_key",
        4
    ));

    let additional_document = parse_pubspec_configuration(PubspecInput::new(
        "config\\pubspec.yaml",
        concat!(
            "name: demo\n",
            "flutter:\n",
            "  generate: true\n",
            "---\n",
            "name: ignored\n",
            "flutter:\n",
            "  generate: false\n",
        ),
    ));
    assert_eq!(
        additional_document.flutter.generate_localizations,
        Some(true)
    );
    assert!(has_diagnostic_at(
        &additional_document.diagnostics,
        "pubspec_multiple_documents_unsupported",
        4,
    ));

    let invalid_selectors = parse_pubspec_configuration(PubspecInput::new(
        "config\\pubspec.yaml",
        concat!(
            "name: demo\n",
            "flutter:\n",
            "  assets:\n",
            "    - path: assets/fuchsia.bin\n",
            "      flavors: ['']\n",
            "      platforms: [fuchsia]\n",
        ),
    ));
    assert!(has_diagnostic(
        &invalid_selectors.diagnostics,
        "pubspec_invalid_flutter_asset_flavor",
    ));
    assert!(has_diagnostic(
        &invalid_selectors.diagnostics,
        "pubspec_invalid_flutter_asset_platform",
    ));
}

#[test]
fn preserves_dependency_source_and_section_contract() {
    let analysis = parse_pubspec(PubspecInput::new(
        "config\\pubspec.yaml",
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
    ));

    assert_eq!(analysis.path, "config/pubspec.yaml");
    assert_eq!(analysis.package_name.as_deref(), Some("demo"));
    assert_eq!(
        dependency_contract(&analysis),
        vec![
            (
                "flutter",
                PubspecDependencySection::Dependencies,
                Some("sdk:flutter")
            ),
            (
                "local_package",
                PubspecDependencySection::Dependencies,
                Some("path:../local_package"),
            ),
            (
                "remote_package",
                PubspecDependencySection::Dependencies,
                Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0"),
            ),
            (
                "hosted_package",
                PubspecDependencySection::Dependencies,
                Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0"),
            ),
            (
                "workspace_package",
                PubspecDependencySection::Dependencies,
                Some("workspace"),
            ),
            (
                "test",
                PubspecDependencySection::DevDependencies,
                Some("^1.25.0")
            ),
            (
                "local_package",
                PubspecDependencySection::DependencyOverrides,
                Some("path:../override"),
            ),
        ]
    );
    assert!(analysis.diagnostics.is_empty());
}

#[test]
fn preserves_inline_wildcard_duplicate_and_unicode_contracts() {
    let inline = parse_pubspec(PubspecInput::new(
        "pubspec.yaml",
        concat!(
            "name: demo\n",
            "dependencies:\n",
            "  local_package: { path: ../local_package }\n",
            "  remote_package: { git: { url: \"https://example.com/repo.git?parts=one,two\", ref: stable }, version: ^1.0.0 }\n",
            "  hosted_package: { hosted: { name: hosted_package, url: https://pub.example.com }, version: ^2.0.0 }\n",
            "  workspace_package: { workspace: true }\n",
        ),
    ));
    assert_eq!(
        dependency_contract(&inline),
        vec![
            (
                "local_package",
                PubspecDependencySection::Dependencies,
                Some("path:../local_package"),
            ),
            (
                "remote_package",
                PubspecDependencySection::Dependencies,
                Some(
                    "git:ref=stable;url=https://example.com/repo.git?parts=one,two;version=^1.0.0"
                ),
            ),
            (
                "hosted_package",
                PubspecDependencySection::Dependencies,
                Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0"),
            ),
            (
                "workspace_package",
                PubspecDependencySection::Dependencies,
                Some("workspace"),
            ),
        ]
    );
    assert!(inline.diagnostics.is_empty());

    let unicode_source = concat!(
        "name: демо\r\n",
        "description: Привет\r\n",
        "dependencies:\r\n",
        "    any_version: *\r\n",
        "    пакет: ^1.2.0\r\n",
    );
    let unicode = parse_pubspec(PubspecInput::new("pubspec.yaml", unicode_source));
    assert_eq!(unicode.package_name.as_deref(), Some("демо"));
    assert_eq!(
        unicode.dependencies[0].version_or_source.as_deref(),
        Some("*")
    );
    assert_eq!(unicode.dependencies[1].name, "пакет");
    assert_eq!(
        unicode.dependencies[1].span.byte_start,
        unicode_source
            .find("пакет")
            .expect("Unicode dependency key")
    );
    assert!(!has_diagnostic(
        &unicode.diagnostics,
        "pubspec_unsupported_yaml_alias",
    ));

    let duplicate = parse_pubspec(PubspecInput::new(
        "pubspec.yaml",
        concat!(
            "name: demo\n",
            "dependencies:\n",
            "  http: ^1.2.0\n",
            "  http: ^1.3.0\n",
        ),
    ));
    assert_eq!(duplicate.dependencies.len(), 2);
    assert!(has_diagnostic_at(
        &duplicate.diagnostics,
        "pubspec_duplicate_key",
        4,
    ));
}

#[test]
fn preserves_crlf_and_unicode_asset_evidence() {
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

    let analysis = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));

    assert_eq!(analysis.flutter.asset_configurations.len(), 1);
    assert_eq!(
        analysis.flutter.asset_configurations[0].span.byte_start,
        expected_byte_start
    );
    assert!(analysis.diagnostics.is_empty());
}

fn dependency_contract(
    analysis: &dartscope_core::PubspecAnalysis,
) -> Vec<(&str, PubspecDependencySection, Option<&str>)> {
    analysis
        .dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.name.as_str(),
                dependency.section,
                dependency.version_or_source.as_deref(),
            )
        })
        .collect()
}

fn has_diagnostic(diagnostics: &[dartscope_core::DartDiagnostic], code: &str) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.code == code)
}

fn has_diagnostic_at(
    diagnostics: &[dartscope_core::DartDiagnostic],
    code: &str,
    line: usize,
) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.code == code
            && diagnostic
                .span
                .as_ref()
                .is_some_and(|span| span.start_line == line)
    })
}
