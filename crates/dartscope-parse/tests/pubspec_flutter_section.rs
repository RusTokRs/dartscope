//! The `flutter:` section is optional content: the Flutter tool accepts it empty or null, so an empty
//! section must not be reported as invalid, while a section of the wrong type still is. A leading
//! byte-order mark must not hide the first key either.

use dartscope_core::{DiagnosticSeverity, PubspecAnalysis, PubspecInput};
use dartscope_parse::{parse_pubspec, parse_pubspec_configuration};

const INVALID_FLUTTER: &str = "pubspec_invalid_flutter_configuration";

fn errors(analysis: &PubspecAnalysis) -> Vec<&str> {
    analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn codes(source: &str) -> Vec<String> {
    parse_pubspec(PubspecInput::new("pubspec.yaml", source))
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn an_empty_or_null_flutter_section_is_not_reported() {
    for source in [
        "name: demo\nflutter:\n",
        "name: demo\nflutter:",
        "name: demo\nflutter: ~\n",
        "name: demo\nflutter: null\n",
        "name: demo\nflutter: Null\n",
        "name: demo\nflutter: NULL\n",
        "name: demo\nflutter:\ndependencies:\n  http: ^1.0.0\n",
    ] {
        let codes = codes(source);
        assert!(
            !codes.iter().any(|code| code == INVALID_FLUTTER),
            "{source:?} -> {codes:?}"
        );
        let configuration = parse_pubspec_configuration(PubspecInput::new("pubspec.yaml", source));
        assert!(configuration.flutter.assets.is_empty(), "{source:?}");
        assert!(configuration.flutter.fonts.is_empty(), "{source:?}");
    }
}

#[test]
fn a_flutter_section_of_the_wrong_type_is_still_invalid() {
    for source in [
        "name: demo\nflutter: 5\n",
        "name: demo\nflutter: yes please\n",
        "name: demo\nflutter:\n  - one\n",
    ] {
        let codes = codes(source);
        assert!(
            codes.iter().any(|code| code == INVALID_FLUTTER),
            "{source:?} -> {codes:?}"
        );
    }
}

#[test]
fn a_populated_flutter_section_is_still_parsed() {
    let analysis = parse_pubspec(PubspecInput::new(
        "pubspec.yaml",
        "name: demo\nflutter:\n  uses-material-design: true\n  assets:\n    - assets/a.png\n",
    ));

    assert_eq!(
        analysis.configuration.flutter.uses_material_design,
        Some(true)
    );
    assert_eq!(analysis.configuration.flutter.assets.len(), 1);
    assert!(errors(&analysis).is_empty(), "{:?}", analysis.diagnostics);
}

#[test]
fn a_byte_order_mark_does_not_hide_the_first_key() {
    let source = "\u{feff}name: demo\nflutter:\n  uses-material-design: true\ndependencies:\n  http: ^1.0.0\n";
    let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

    assert_eq!(analysis.package_name.as_deref(), Some("demo"));
    assert_eq!(
        analysis.configuration.flutter.uses_material_design,
        Some(true)
    );
    assert_eq!(
        analysis.dependencies.len(),
        1,
        "{:?}",
        analysis.dependencies
    );
    let dependency = &analysis.dependencies[0];
    assert_eq!(dependency.name, "http");
    assert_eq!(dependency.span.start_line, 5);
    assert!(
        source[dependency.span.byte_start..dependency.span.byte_end].contains("http"),
        "the span must point into the original text, which starts with the 3-byte mark: {:?}",
        dependency.span
    );
    assert!(errors(&analysis).is_empty(), "{:?}", analysis.diagnostics);
}

#[test]
fn a_byte_order_mark_before_a_document_marker_is_ignored() {
    let analysis = parse_pubspec(PubspecInput::new(
        "pubspec.yaml",
        "\u{feff}---\nname: demo\n",
    ));

    assert_eq!(analysis.package_name.as_deref(), Some("demo"));
}
