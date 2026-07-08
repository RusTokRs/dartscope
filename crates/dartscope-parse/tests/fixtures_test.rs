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
    assert!(analysis
        .flutter
        .assets
        .iter()
        .any(|asset| asset.path == "assets/images/logo.png"));
    assert!(analysis
        .flutter
        .assets
        .iter()
        .any(|asset| asset.path == "assets/config/app.json"));
    assert!(analysis
        .flutter
        .localizations
        .iter()
        .any(|localization| localization.key == "homeTitle"));

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
    assert_eq!(analysis.summary.imports, 4);
    assert_eq!(analysis.summary.exports, 1);
    assert_eq!(analysis.summary.parts, 1);
    assert_eq!(analysis.summary.flutter_widgets, 2);
    assert_eq!(analysis.summary.flutter_assets, 2);
    assert_eq!(analysis.summary.flutter_localizations, 1);
    assert_eq!(analysis.summary.package_dependencies, 3);
    assert_eq!(analysis.summary.diagnostics, analysis.diagnostics.len());
}

#[test]
fn navigation_fixture_reports_go_routes_with_resolved_paths() {
    let source = include_str!("fixtures/flutter_app/lib/navigation.dart");
    let analysis = analyze_file(DartFileInput::new("lib/navigation.dart", source));

    assert!(analysis.flutter.imports_flutter);

    // Three GoRoute hints should be detected.
    let routes = &analysis.flutter.routes;
    assert_eq!(routes.len(), 3, "expected 3 GoRoute hints, got {routes:?}");

    // All routes use GoRoute constructor.
    assert!(routes.iter().all(|r| r.constructor == "GoRoute"));

    // Home route: path constant resolves to '/'.
    let home_route = routes
        .iter()
        .find(|r| r.resolved_path.as_deref() == Some("/"))
        .expect("home route with resolved_path='/' not found");
    assert_eq!(home_route.name.as_deref(), Some("home"));

    // Settings route: path constant resolves to '/settings'.
    let settings_route = routes
        .iter()
        .find(|r| r.resolved_path.as_deref() == Some("/settings"))
        .expect("settings route with resolved_path='/settings' not found");
    assert_eq!(settings_route.name.as_deref(), Some("settings"));

    // Profile route: path constant resolves to '/profile/:id'.
    assert!(
        routes
            .iter()
            .any(|r| r.resolved_path.as_deref() == Some("/profile/:id")),
        "profile route with resolved_path='/profile/:id' not found"
    );

    // Three string constants for route paths.
    assert!(
        analysis
            .string_constants
            .iter()
            .any(|c| c.name == "homeRoute" && c.value == "/"),
        "homeRoute constant not found"
    );
    assert!(
        analysis
            .string_constants
            .iter()
            .any(|c| c.name == "settingsRoute" && c.value == "/settings"),
        "settingsRoute constant not found"
    );
    assert!(
        analysis
            .string_constants
            .iter()
            .any(|c| c.name == "profileRoute" && c.value == "/profile/:id"),
        "profileRoute constant not found"
    );

    // Three widget classes.
    let widgets = &analysis.flutter.widgets;
    assert_eq!(widgets.len(), 3, "expected 3 widget hints, got {widgets:?}");
    assert!(widgets.iter().any(|w| w.class_name == "HomeScreen"));
    assert!(widgets.iter().any(|w| w.class_name == "SettingsScreen"));
    assert!(widgets.iter().any(|w| w.class_name == "ProfileScreen"));
}
