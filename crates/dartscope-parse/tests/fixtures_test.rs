use dartscope_core::{DartFileInput, DartProjectInput, PubspecDependencySection, PubspecInput};
use dartscope_parse::{analyze_file, analyze_project, parse_pubspec};

#[test]
fn flutter_fixture_reports_widgets_and_dependencies() {
    let source = include_str!("fixtures/flutter_app/lib/main.dart");
    let analysis = analyze_file(DartFileInput::new("lib/main.dart", source));

    assert!(analysis.flutter.imports_flutter);
    assert!(analysis
        .flutter
        .widgets
        .iter()
        .any(|widget| widget.class_name == "HomeScreen"));
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "LabelBuilder"));

    let pubspec = include_str!("fixtures/flutter_app/pubspec.yaml");
    let pubspec = parse_pubspec(PubspecInput::new("pubspec.yaml", pubspec));

    assert_eq!(pubspec.package_name.as_deref(), Some("fixture_flutter_app"));
    assert!(pubspec.dependencies.iter().any(|dependency| {
        dependency.name == "http" && dependency.section == PubspecDependencySection::Dependencies
    }));
}

#[test]
fn pure_dart_fixture_does_not_emit_flutter_widgets() {
    let source = include_str!("fixtures/pure_dart/lib/math.dart");
    let analysis = analyze_file(DartFileInput::new("lib/math.dart", source));

    assert!(!analysis.flutter.imports_flutter);
    assert!(analysis.flutter.widgets.is_empty());
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "Calculator"));
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "add"));
}

#[test]
fn flutter_fixture_project_summary_is_stable() {
    let main = include_str!("fixtures/flutter_app/lib/main.dart");
    let pubspec = include_str!("fixtures/flutter_app/pubspec.yaml");

    let analysis = analyze_project(DartProjectInput::new(
        "fixtures/flutter_app",
        vec![DartFileInput::new("lib\\main.dart", main)],
        vec![PubspecInput::new("pubspec.yaml", pubspec)],
    ));

    assert_eq!(analysis.root, "fixtures/flutter_app");
    assert_eq!(analysis.summary.dart_files, 1);
    assert_eq!(analysis.summary.pubspecs, 1);
    assert_eq!(analysis.summary.imports, 2);
    assert_eq!(analysis.summary.exports, 1);
    assert_eq!(analysis.summary.parts, 1);
    assert_eq!(analysis.summary.flutter_widgets, 2);
    assert_eq!(analysis.summary.package_dependencies, 3);
    assert_eq!(analysis.summary.diagnostics, analysis.diagnostics.len());
}
