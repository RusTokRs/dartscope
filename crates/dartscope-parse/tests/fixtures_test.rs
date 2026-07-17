use dartscope_core::{
    DartFileInput, DartProjectInput, FlutterFileHints, PubspecDependencySection, PubspecInput,
};
use dartscope_parse::{analyze_file, analyze_project, parse_pubspec};

#[test]
fn flutter_fixture_reports_generic_dart_facts_and_dependencies() {
    let source = include_str!("fixtures/flutter_app/lib/main.dart");
    let analysis = analyze_file(DartFileInput::new("lib/main.dart", source));

    assert_eq!(analysis.flutter, FlutterFileHints::default());
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "HomeScreen"
            && declaration.extends.as_deref() == Some("StatelessWidget")
    }));
    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "LabelBuilder")
    );
    assert!(analysis.invocations.iter().any(|invocation| {
        invocation.target == "Image.asset"
            && invocation.arguments[0].string_value.as_deref() == Some("assets/images/logo.png")
    }));
    assert!(analysis.invocations.iter().any(|invocation| {
        invocation.target == "rootBundle.loadString"
            && invocation.arguments[0].string_value.as_deref() == Some("assets/config/app.json")
    }));
    assert!(analysis.invocations.iter().any(|invocation| {
        invocation.target == "AppLocalizations.of" && invocation.result_members == ["homeTitle"]
    }));

    let pubspec = include_str!("fixtures/flutter_app/pubspec.yaml");
    let pubspec = parse_pubspec(PubspecInput::new("pubspec.yaml", pubspec));

    assert_eq!(pubspec.package_name.as_deref(), Some("fixture_flutter_app"));
    assert!(pubspec.dependencies.iter().any(|dependency| {
        dependency.name == "http" && dependency.section == PubspecDependencySection::Dependencies
    }));
}

#[test]
fn pure_dart_fixture_does_not_emit_flutter_conventions() {
    let source = include_str!("fixtures/pure_dart/lib/math.dart");
    let analysis = analyze_file(DartFileInput::new("lib/math.dart", source));

    assert_eq!(analysis.flutter, FlutterFileHints::default());
    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "Calculator")
    );
    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "add")
    );
}

#[test]
fn pure_parser_project_summary_is_stable() {
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
    assert_eq!(analysis.summary.flutter_widgets, 0);
    assert_eq!(analysis.summary.flutter_assets, 0);
    assert_eq!(analysis.summary.flutter_localizations, 0);
    assert_eq!(analysis.summary.package_dependencies, 3);
    assert_eq!(analysis.summary.diagnostics, analysis.diagnostics.len());
}

#[test]
fn navigation_fixture_reports_generic_go_route_invocations() {
    let source = include_str!("fixtures/flutter_app/lib/navigation.dart");
    let analysis = analyze_file(DartFileInput::new("lib/navigation.dart", source));

    assert_eq!(analysis.flutter, FlutterFileHints::default());
    let routes: Vec<_> = analysis
        .invocations
        .iter()
        .filter(|invocation| invocation.target == "GoRoute")
        .collect();
    assert_eq!(
        routes.len(),
        3,
        "expected 3 GoRoute invocations, got {routes:?}"
    );
    assert!(routes.iter().all(|route| {
        route
            .arguments
            .iter()
            .any(|argument| argument.name.as_deref() == Some("path"))
    }));

    assert!(
        analysis
            .string_constants
            .iter()
            .any(|constant| constant.name == "homeRoute" && constant.value == "/")
    );
    assert!(
        analysis
            .string_constants
            .iter()
            .any(|constant| { constant.name == "settingsRoute" && constant.value == "/settings" })
    );
    assert!(
        analysis.string_constants.iter().any(|constant| {
            constant.name == "profileRoute" && constant.value == "/profile/:id"
        })
    );

    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "HomeScreen")
    );
    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "SettingsScreen")
    );
    assert!(
        analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "ProfileScreen")
    );
}
