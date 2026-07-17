use dartscope_core::{DartFileInput, FlutterRoutePathKind};
use dartscope_flutter::derive_flutter_file_hints;
use dartscope_parse::analyze_file;

#[test]
fn derives_official_application_and_navigator_named_routes() {
    let source = r#"import 'package:flutter/widgets.dart' as fw;

const settingsRoute = '/settings';

void buildApp(BuildContext context) {
  fw.WidgetsApp(
    initialRoute: '/home',
    routes: <String, WidgetBuilder>{
      '/home': homeBuilder,
      '/profile': profileBuilder,
    },
    color: const Color(0xff000000),
  );
  fw.Navigator(
    initialRoute: '/nested',
    onGenerateRoute: nestedRouteFactory,
  );
  fw.Navigator.pushNamed(context, settingsRoute);
  fw.Navigator.of(context).pushReplacementNamed('/login');
  fw.Navigator.restorablePopAndPushNamed(context, '/restored');
}
"#;
    let file = analyze_file(DartFileInput::new("lib/app.dart", source));
    let hints = derive_flutter_file_hints(&file);

    let constructors: Vec<_> = hints
        .routes
        .iter()
        .map(|route| route.constructor.as_str())
        .collect();
    assert_eq!(
        constructors,
        [
            "WidgetsApp.initialRoute",
            "WidgetsApp.routes",
            "WidgetsApp.routes",
            "Navigator.initialRoute",
            "Navigator.pushNamed",
            "Navigator.of.pushReplacementNamed",
            "Navigator.restorablePopAndPushNamed",
        ]
    );
    assert_eq!(hints.routes[0].resolved_path.as_deref(), Some("/home"));
    assert_eq!(hints.routes[3].resolved_path.as_deref(), Some("/nested"));
    assert_eq!(hints.routes[4].path, "settingsRoute");
    assert_eq!(hints.routes[4].resolved_path.as_deref(), Some("/settings"));
    assert_eq!(hints.routes[4].path_kind, FlutterRoutePathKind::Expression);
    assert_eq!(hints.routes[5].resolved_path.as_deref(), Some("/login"));
    assert!(
        hints
            .routes
            .iter()
            .all(|route| route.span.byte_end > route.span.byte_start)
    );
}

#[test]
fn derives_material_routes_and_rejects_unrelated_named_navigation() {
    let source = r#"import 'package:flutter/material.dart';

void buildApp(BuildContext context) {
  MaterialApp(
    home: dashboardBuilder,
    initialRoute: '/dashboard',
    routes: {'/dashboard': dashboardBuilder},
  );
  Router.pushNamed(context, '/not-flutter-navigator');
  Navigator.push(context, MaterialPageRoute(builder: dashboardBuilder));
}
"#;
    let file = analyze_file(DartFileInput::new("lib/material_app.dart", source));
    let hints = derive_flutter_file_hints(&file);

    assert_eq!(hints.routes.len(), 3);
    assert_eq!(hints.routes[0].constructor, "MaterialApp.home");
    assert_eq!(hints.routes[0].resolved_path.as_deref(), Some("/"));
    assert_eq!(hints.routes[1].constructor, "MaterialApp.initialRoute");
    assert_eq!(hints.routes[2].constructor, "MaterialApp.routes");
    assert!(
        hints
            .routes
            .iter()
            .all(|route| !route.path.contains("not-flutter-navigator"))
    );
}

#[test]
fn official_route_names_require_an_official_flutter_import() {
    let source = r#"void open(Object context) {
  Navigator.pushNamed(context, '/false-positive');
  WidgetsApp(routes: {'/fake': fakeBuilder});
}
"#;
    let file = analyze_file(DartFileInput::new("lib/plain.dart", source));
    let hints = derive_flutter_file_hints(&file);

    assert!(hints.routes.is_empty());
}
