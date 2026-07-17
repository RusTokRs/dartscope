from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one replacement in {path}, found {count}")
    file.write_text(text.replace(old, new), encoding="utf-8")


CONVENTIONS = "crates/dartscope-flutter/src/conventions.rs"

replace_once(
    CONVENTIONS,
    """        hints.routes.extend(route_hints(invocation, &constants));
""",
    """        hints.routes.extend(route_hints(
            invocation,
            &constants,
            imports_official_flutter(file),
        ));
""",
)

replace_once(
    CONVENTIONS,
    """fn route_hints(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
) -> Vec<FlutterRouteHint> {
    match invocation.target.as_str() {
        "GoRoute" => go_route_hint(invocation, constants).into_iter().collect(),
        "MaterialApp" => material_routes(invocation),
        _ => Vec::new(),
    }
}
""",
    """fn route_hints(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
    imports_official_flutter: bool,
) -> Vec<FlutterRouteHint> {
    if invocation.target == "GoRoute" {
        return go_route_hint(invocation, constants).into_iter().collect();
    }
    if !imports_official_flutter {
        return Vec::new();
    }
    if let Some(application) = official_application(&invocation.target) {
        return application_route_hints(invocation, constants, application);
    }
    navigator_route_hint(invocation, constants)
        .into_iter()
        .collect()
}
""",
)

replace_once(
    CONVENTIONS,
    """fn material_routes(invocation: &DartInvocation) -> Vec<FlutterRouteHint> {
    let Some(routes) = named_argument(invocation, "routes") else {
        return Vec::new();
    };
    routes
        .map_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.string_key.clone()?;
            Some(FlutterRouteHint {
                constructor: "MaterialApp.routes".to_string(),
                path: path.clone(),
                path_kind: FlutterRoutePathKind::Literal,
                resolved_path: Some(path),
                name: None,
                confidence: Confidence::High,
                span: entry.source_line_span.clone(),
            })
        })
        .collect()
}
""",
    """fn official_application(target: &str) -> Option<&'static str> {
    match target.rsplit('.').next()? {
        "MaterialApp" => Some("MaterialApp"),
        "WidgetsApp" => Some("WidgetsApp"),
        "Navigator" => Some("Navigator"),
        _ => None,
    }
}

fn application_route_hints(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
    application: &str,
) -> Vec<FlutterRouteHint> {
    let mut hints = if application == "Navigator" {
        Vec::new()
    } else {
        route_table_hints(invocation, application)
    };
    if application != "Navigator"
        && let Some(home) = named_argument(invocation, "home")
    {
        hints.push(FlutterRouteHint {
            constructor: format!("{application}.home"),
            path: "/".to_string(),
            path_kind: FlutterRoutePathKind::Literal,
            resolved_path: Some("/".to_string()),
            name: None,
            confidence: Confidence::High,
            span: home.span.clone(),
        });
    }
    if let Some(initial_route) = named_argument(invocation, "initialRoute") {
        hints.push(route_hint_from_argument(
            format!("{application}.initialRoute"),
            initial_route,
            constants,
        ));
    }
    hints
}

fn route_table_hints(invocation: &DartInvocation, application: &str) -> Vec<FlutterRouteHint> {
    let Some(routes) = named_argument(invocation, "routes") else {
        return Vec::new();
    };
    routes
        .map_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.string_key.clone()?;
            Some(FlutterRouteHint {
                constructor: format!("{application}.routes"),
                path: path.clone(),
                path_kind: FlutterRoutePathKind::Literal,
                resolved_path: Some(path),
                name: None,
                confidence: Confidence::High,
                span: entry.source_line_span.clone(),
            })
        })
        .collect()
}

fn navigator_route_hint(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
) -> Option<FlutterRouteHint> {
    let route_argument_index = navigator_route_argument_index(&invocation.target)?;
    let route = positional_argument(invocation, route_argument_index)?;
    Some(route_hint_from_argument(
        canonical_navigator_constructor(&invocation.target),
        route,
        constants,
    ))
}

fn navigator_route_argument_index(target: &str) -> Option<usize> {
    let method = target.rsplit('.').next()?;
    if !matches!(
        method,
        "pushNamed"
            | "pushReplacementNamed"
            | "pushNamedAndRemoveUntil"
            | "popAndPushNamed"
            | "restorablePushNamed"
            | "restorablePushReplacementNamed"
            | "restorablePushNamedAndRemoveUntil"
            | "restorablePopAndPushNamed"
    ) {
        return None;
    }
    let parts: Vec<_> = target.split('.').collect();
    let navigator = parts.iter().rposition(|part| *part == "Navigator")?;
    match parts.get(navigator + 1..) {
        Some([_, _]) if parts[navigator + 1] == "of" => Some(0),
        Some([_]) => Some(1),
        _ => None,
    }
}

fn canonical_navigator_constructor(target: &str) -> String {
    let method = target.rsplit('.').next().unwrap_or(target);
    if target.split('.').any(|part| part == "of") {
        format!("Navigator.of.{method}")
    } else {
        format!("Navigator.{method}")
    }
}

fn route_hint_from_argument(
    constructor: String,
    argument: &DartInvocationArgument,
    constants: &HashMap<&str, &str>,
) -> FlutterRouteHint {
    let route_path = route_path_value(argument, constants);
    FlutterRouteHint {
        constructor,
        path: route_path.value,
        path_kind: route_path.kind,
        resolved_path: route_path.resolved,
        name: None,
        confidence: route_path.confidence,
        span: argument.span.clone(),
    }
}
""",
)

replace_once(
    CONVENTIONS,
    """fn is_flutter_import(uri: &str) -> bool {
    uri.starts_with("package:flutter/") || uri.starts_with("package:flutter_riverpod/")
}
""",
    """fn imports_official_flutter(file: &DartFileAnalysis) -> bool {
    file.imports
        .iter()
        .any(|import| is_official_flutter_import(&import.uri))
}

fn is_official_flutter_import(uri: &str) -> bool {
    uri.starts_with("package:flutter/")
}

fn is_flutter_import(uri: &str) -> bool {
    is_official_flutter_import(uri) || uri.starts_with("package:flutter_riverpod/")
}
""",
)

TEST = r'''use dartscope_core::{DartFileInput, FlutterRoutePathKind};
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
    assert!(hints.routes.iter().all(|route| route.span.byte_end > route.span.byte_start));
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
'''
Path("crates/dartscope-flutter/tests/official_navigation.rs").write_text(TEST, encoding="utf-8")

replace_once(
    "docs/development/dartscope-library-plan.md",
    "Status: ready. Priority: P2. Prerequisite: DS-FLUTTER-002.",
    "Status: in_progress. Priority: P2. Prerequisite: DS-FLUTTER-002.",
)
replace_once(
    "docs/development/dartscope-library-plan.md",
    """4. Keep package conventions in `dartscope-flutter`; do not reinterpret application-specific
   manifests or move framework semantics into the pure parser.

Acceptance:
""",
    """4. Keep package conventions in `dartscope-flutter`; do not reinterpret application-specific
   manifests or move framework semantics into the pure parser.

Progress (2026-07-17):

- [x] Derive official `MaterialApp` and `WidgetsApp` home/default routes, route tables, and
  `initialRoute` facts, plus `Navigator.initialRoute`.
- [x] Derive official static and `Navigator.of(context)` named-route navigation calls, including
  restorable variants, with constant resolution and source spans.
- [ ] Normalize supported theme construction and application facts.
- [ ] Define versioned opt-in `go_router` and state-management support metadata and fixtures.

Acceptance:
""",
)

replace_once(
    "README.md",
    """- `dartscope-flutter` derives widget, route, asset, and localization conventions from generic
  imports, declarations, and invocations, aggregates project-level inventory, links direct asset
""",
    """- `dartscope-flutter` derives widget, official application-route and named-navigation, asset, and
  localization conventions from generic imports, declarations, and invocations, aggregates
  project-level inventory, links direct asset
""",
)
replace_once(
    "README.md",
    """widget hints, `GoRoute` hints with `resolved_path` when a route path can be resolved
from same-file string constants, and high-confidence direct Flutter asset/localization
""",
    """widget hints, official `MaterialApp`/`WidgetsApp` route tables and named `Navigator` calls,
legacy `GoRoute` hints with `resolved_path` when a route path can be resolved from same-file string
constants, and high-confidence direct Flutter asset/localization
""",
)
