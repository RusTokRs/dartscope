use dartscope_core::{DartFileInput, PubspecDependencySection, PubspecInput};
use dartscope_parse::{analyze_file, parse_pubspec};

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

