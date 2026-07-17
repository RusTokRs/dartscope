use dartscope_core::{DartFileInput, DartProjectInput, FlutterFileHints};
use dartscope_flutter::{extract_flutter_inventory, populate_flutter_project_analysis};
use dartscope_parse::{analyze_file, analyze_project};

#[test]
fn optional_convention_layer_restores_flutter_fixture_findings() {
    let source = include_str!("../../dartscope-parse/tests/fixtures/flutter_app/lib/main.dart");
    let analysis = analyze_file(DartFileInput::new("lib/main.dart", source));

    assert_eq!(analysis.flutter, FlutterFileHints::default());
    let project = DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new("lib/main.dart", source)],
        Vec::new(),
    );
    let inventory = extract_flutter_inventory(&analyze_project(project));

    assert_eq!(inventory.flutter_file_paths, ["lib/main.dart"]);
    assert!(inventory.widgets.iter().any(|widget| {
        widget.class_name == "HomeScreen" && widget.base_class == "StatelessWidget"
    }));
    assert!(
        inventory
            .widgets
            .iter()
            .any(|widget| { widget.class_name == "CounterState" && widget.base_class == "State" })
    );
    assert!(
        inventory
            .assets
            .iter()
            .any(|asset| asset.asset_path == "assets/images/logo.png")
    );
    assert!(
        inventory
            .assets
            .iter()
            .any(|asset| asset.asset_path == "assets/config/app.json")
    );
    assert!(
        inventory
            .localizations
            .iter()
            .any(|localization| localization.key == "homeTitle")
    );
    assert!(
        inventory
            .assets
            .iter()
            .all(|asset| asset.span.start_line == asset.span.end_line)
    );
}

#[test]
fn go_router_fixture_keeps_resolution_order_and_source_evidence() {
    let source =
        include_str!("../../dartscope-parse/tests/fixtures/flutter_app/lib/navigation.dart");
    let project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new("lib/navigation.dart", source)],
        Vec::new(),
    ));
    let inventory = extract_flutter_inventory(&project);

    assert_eq!(inventory.routes.len(), 3);
    assert_eq!(
        inventory
            .routes
            .iter()
            .filter_map(|route| route.resolved_path.as_deref())
            .collect::<Vec<_>>(),
        ["/", "/settings", "/profile/:id"]
    );
    assert_eq!(inventory.routes[0].name.as_deref(), Some("home"));
    assert_eq!(inventory.routes[1].name.as_deref(), Some("settings"));
    assert!(
        inventory
            .routes
            .windows(2)
            .all(|pair| pair[0].span.byte_start < pair[1].span.byte_start)
    );
    assert_eq!(inventory.widgets.len(), 3);
}

#[test]
fn explicit_project_composition_populates_v1_compatibility_projection() {
    let main = include_str!("../../dartscope-parse/tests/fixtures/flutter_app/lib/main.dart");
    let navigation =
        include_str!("../../dartscope-parse/tests/fixtures/flutter_app/lib/navigation.dart");
    let mut project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![
            DartFileInput::new("lib/main.dart", main),
            DartFileInput::new("lib/navigation.dart", navigation),
        ],
        Vec::new(),
    ));

    assert_eq!(project.summary.flutter_widgets, 0);
    populate_flutter_project_analysis(&mut project);

    assert_eq!(project.summary.flutter_widgets, 5);
    assert_eq!(project.summary.flutter_routes, 3);
    assert_eq!(project.summary.flutter_assets, 2);
    assert_eq!(project.summary.flutter_localizations, 1);
    assert!(
        project
            .files
            .iter()
            .all(|file| file.flutter.imports_flutter)
    );
}
