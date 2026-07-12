use crate::{analyze_file, analyze_project, parse_pubspec};
use dartscope_core::*;

#[test]
fn analyzes_dart_imports_parts_declarations_and_flutter_widgets() {
    let source = r#"
import 'package:flutter/material.dart';
import 'src/model.dart';
export "src/api.dart";
part 'home.g.dart';

class HomeScreen extends StatelessWidget {
}

typedef Mapper = String Function(int value);
"#;

    let analysis = analyze_file(DartFileInput::new("lib\\home.dart", source));

    assert_eq!(analysis.path, "lib/home.dart");
    assert_eq!(analysis.imports.len(), 2);
    assert_eq!(analysis.exports[0].uri, "src/api.dart");
    assert_eq!(analysis.parts[0].uri, "home.g.dart");
    assert!(analysis.flutter.imports_flutter);
    assert_eq!(analysis.flutter.widgets[0].class_name, "HomeScreen");
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "Mapper"
            && declaration.kind == DartDeclarationKind::Typedef));
}

#[test]
fn keeps_byte_spans_exact_for_crlf_sources_and_attributes_diagnostics() {
    let first_line = "const operation = r'''query Demo { demo }''';";
    let second_line = "client.query(QueryOptions(document: gql(operation)));";
    let source = format!("{first_line}\r\n{second_line}\r\npart 'missing.dart'");

    let analysis = analyze_file(DartFileInput::new("lib\\api.dart", source));

    assert_eq!(analysis.graphql_operations[0].span.byte_start, 0);
    assert_eq!(
        analysis.graphql_operation_uses[0].span.byte_start,
        first_line.len() + 2
    );
    assert_eq!(analysis.parts[0].span.start_line, 3);
    assert_eq!(
        analysis.parts[0].span.byte_start,
        first_line.len() + second_line.len() + 4
    );
    assert_eq!(
        analysis.diagnostics[0].path.as_deref(),
        Some("lib/api.dart")
    );
}

#[test]
fn ignores_parser_markers_inside_comments_and_strings_for_lf_and_crlf_sources() {
    let source = "// import 'fake.dart'; class Commented {} Image.asset('fake.png')\r\n\
const sample = \"class StringValue {} GoRoute(path: '/fake')\";\r\n\
/* AppLocalizations.of(context)!.fakeKey\n   import 'also_fake.dart'; */\r\n\
import 'package:flutter/widgets.dart';\r\n\
class RealScreen extends StatelessWidget {}\r\n\
final icon = Image.asset('assets/real.png');\r\n\
final title = AppLocalizations.of(context)!.realTitle;\n";

    let analysis = analyze_file(DartFileInput::new("lib/real.dart", source));

    assert_eq!(analysis.imports.len(), 1);
    assert_eq!(analysis.imports[0].uri, "package:flutter/widgets.dart");
    assert!(analysis
        .declarations
        .iter()
        .any(|item| item.name == "RealScreen"));
    assert!(!analysis
        .declarations
        .iter()
        .any(|item| item.name == "Commented" || item.name == "StringValue"));
    assert_eq!(analysis.flutter.assets.len(), 1);
    assert_eq!(analysis.flutter.assets[0].path, "assets/real.png");
    assert_eq!(analysis.flutter.localizations.len(), 1);
    assert_eq!(analysis.flutter.localizations[0].key, "realTitle");
}

#[test]
fn reports_unterminated_lexical_constructs_with_source_spans() {
    let source = "class Ready {}\n/* nested /* comment\nfinal value = 'unterminated\n";
    let analysis = analyze_file(DartFileInput::new("lib/broken.dart", source));

    assert!(analysis.diagnostics.iter().any(|diagnostic| diagnostic.code
        == "unterminated_block_comment"
        && diagnostic.path.as_deref() == Some("lib/broken.dart")
        && diagnostic
            .span
            .as_ref()
            .is_some_and(|span| span.start_line == 2)));
    assert!(!analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "unterminated_string"));

    let string_analysis = analyze_file(DartFileInput::new(
        "lib/string.dart",
        "final value = 'unterminated\nclass Recovered {}\n",
    ));
    assert!(string_analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "unterminated_string"
            && diagnostic
                .span
                .as_ref()
                .is_some_and(|span| span.start_line == 1)));
    assert!(string_analysis
        .declarations
        .iter()
        .any(|item| item.name == "Recovered"));
}

#[test]
fn parses_modern_type_declarations_without_inventing_unnamed_extensions() {
    let source = r#"
import 'package:flutter/widgets.dart' as widgets;

abstract base class Service {}
mixin class Reusable {}
base mixin InternalMixin {}
extension on String {}
extension Parsing on String {}
extension type const UserId(int value) {}
class HomeScreen extends widgets.StatelessWidget {}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/types.dart", source));

    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "Service" && declaration.kind == DartDeclarationKind::Class
    }));
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "Reusable" && declaration.kind == DartDeclarationKind::Class
    }));
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "InternalMixin" && declaration.kind == DartDeclarationKind::Mixin
    }));
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "Parsing" && declaration.kind == DartDeclarationKind::Extension
    }));
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "UserId" && declaration.kind == DartDeclarationKind::ExtensionType
    }));
    assert!(!analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "on" || declaration.name == "type"));
    assert_eq!(analysis.flutter.widgets[0].class_name, "HomeScreen");
    assert_eq!(
        analysis.flutter.widgets[0].base_class,
        "widgets.StatelessWidget"
    );
}

#[test]
fn extracts_multiple_single_line_routes_and_resolves_forward_constants() {
    let source = r#"
final routes = [
  GoRoute(path: homePath),
  GoRoute(path: '/settings', name: 'settings'),
];

const homePath = '/home';
"#;

    let analysis = analyze_file(DartFileInput::new("lib/router.dart", source));

    assert_eq!(analysis.flutter.routes.len(), 2);
    assert_eq!(analysis.flutter.routes[0].path, "homePath");
    assert_eq!(
        analysis.flutter.routes[0].resolved_path.as_deref(),
        Some("/home")
    );
    assert_eq!(analysis.flutter.routes[1].path, "/settings");
    assert_eq!(analysis.flutter.routes[1].name.as_deref(), Some("settings"));
}

#[test]
fn parses_import_and_export_namespace_controls() {
    let source = r#"
import 'src/generated.dart' as generated show operation, model hide internal;
import 'src/lazy.dart' deferred as lazy;
export 'src/public.dart' show PublicApi hide InternalApi;
"#;

    let analysis = analyze_file(DartFileInput::new("lib/api.dart", source));

    assert_eq!(analysis.imports[0].prefix.as_deref(), Some("generated"));
    assert!(!analysis.imports[0].is_deferred);
    assert_eq!(analysis.imports[0].combinators.len(), 2);
    assert_eq!(
        analysis.imports[0].combinators[0].kind,
        DartNamespaceCombinatorKind::Show
    );
    assert_eq!(
        analysis.imports[0].combinators[0].names,
        ["operation", "model"]
    );
    assert_eq!(analysis.imports[0].combinators[1].names, ["internal"]);
    assert!(analysis.imports[1].is_deferred);
    assert_eq!(analysis.imports[1].prefix.as_deref(), Some("lazy"));
    assert_eq!(analysis.exports[0].combinators.len(), 2);
    assert_eq!(analysis.exports[0].combinators[0].names, ["PublicApi"]);
    assert_eq!(analysis.exports[0].combinators[1].names, ["InternalApi"]);
}

#[test]
fn parses_conditional_imports_and_exports_without_selecting_a_platform() {
    let source = r#"
import 'src/stub.dart'
  if (dart.library.io) 'src/io.dart'
  if (dart.library.js_interop) 'src/web.dart' show PlatformApi;
export 'src/default.dart' if (dart.library.io) 'src/native.dart' hide Internal;
"#;

    let analysis = analyze_file(DartFileInput::new("lib/platform.dart", source));

    assert_eq!(analysis.imports.len(), 1);
    assert_eq!(analysis.imports[0].configurations.len(), 2);
    assert_eq!(analysis.imports[0].span.start_line, 2);
    assert_eq!(analysis.imports[0].span.end_line, 4);
    assert_eq!(analysis.imports[0].combinators[0].names, ["PlatformApi"]);
    assert_eq!(analysis.exports.len(), 1);
    assert_eq!(analysis.exports[0].configurations.len(), 1);
    assert_eq!(
        analysis.exports[0].configurations[0].condition,
        "dart.library.io"
    );
    assert_eq!(analysis.exports[0].configurations[0].uri, "src/native.dart");
    assert_eq!(analysis.exports[0].combinators[0].names, ["Internal"]);
    assert!(analysis.diagnostics.is_empty());

    let single_line = analyze_file(DartFileInput::new(
            "lib/platform_single_line.dart",
            "import 'src/stub.dart' if (dart.library.io) 'src/io.dart' if (dart.library.js_interop) 'src/web.dart' show PlatformApi;",
        ));
    assert_eq!(single_line.imports[0].configurations.len(), 2);
    assert_eq!(
        single_line.imports[0].configurations[1].condition,
        "dart.library.js_interop"
    );
    assert_eq!(single_line.imports[0].configurations[1].uri, "src/web.dart");
    assert_eq!(single_line.imports[0].combinators[0].names, ["PlatformApi"]);
}

#[test]
fn distinguishes_part_of_from_part_directives() {
    let analysis = analyze_file(DartFileInput::new(
        "lib/src/model.dart",
        "part of '../models.dart';\n",
    ));

    assert!(analysis.parts.is_empty());
    assert_eq!(
        analysis.part_of.as_ref().map(|part| part.library.as_str()),
        Some("../models.dart")
    );
    assert_eq!(
        analysis.part_of.as_ref().map(|part| part.kind),
        Some(DartPartOfKind::Uri)
    );
}

#[test]
fn parses_library_name_and_named_part_of_directive() {
    let library = analyze_file(DartFileInput::new(
        "lib/models.dart",
        "library app.models;\npart 'src/model.dart';\n",
    ));
    let part = analyze_file(DartFileInput::new(
        "lib/src/model.dart",
        "part of app.models;\n",
    ));

    assert_eq!(
        library
            .library
            .as_ref()
            .and_then(|value| value.name.as_deref()),
        Some("app.models")
    );
    assert_eq!(
        part.part_of.as_ref().map(|value| value.kind),
        Some(DartPartOfKind::LibraryName)
    );
}

#[test]
fn parses_pubspec_dependencies() {
    let source = r#"
name: demo_app
dependencies:
  flutter:
    sdk: flutter
  http: ^1.2.0
dev_dependencies:
  test: ^1.25.0
"#;

    let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

    assert_eq!(analysis.package_name.as_deref(), Some("demo_app"));
    assert!(analysis.dependencies.iter().any(|dependency| {
        dependency.name == "http"
            && dependency.section == PubspecDependencySection::Dependencies
            && dependency.version_or_source.as_deref() == Some("^1.2.0")
    }));
    assert!(analysis.dependencies.iter().any(|dependency| {
        dependency.name == "test" && dependency.section == PubspecDependencySection::DevDependencies
    }));
}

#[test]
fn parses_indented_pubspec_dependencies_and_ignores_nested_source_fields() {
    let source = r#"
name: 'demo_app' # package name
dependencies: # runtime packages
    flutter:
        sdk: flutter
    http: ^1.2.0 # network client
dev_dependencies:
    test: ^1.25.0
"#;

    let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

    assert_eq!(analysis.package_name.as_deref(), Some("demo_app"));
    assert_eq!(analysis.dependencies.len(), 3);
    assert!(analysis.dependencies.iter().any(|dependency| {
        dependency.name == "http" && dependency.version_or_source.as_deref() == Some("^1.2.0")
    }));
    assert!(!analysis
        .dependencies
        .iter()
        .any(|dependency| dependency.name == "sdk"));
}

#[test]
fn project_analysis_is_sorted_and_project_diagnostics_keep_source_paths() {
    let analysis = analyze_project(DartProjectInput::new(
        "demo",
        vec![
            DartFileInput::new("lib/z.dart", "part 'z.g.dart'"),
            DartFileInput::new("lib/a.dart", "class A {}"),
        ],
        vec![
            PubspecInput::new("packages/z/pubspec.yaml", "dependencies:\n  test: any"),
            PubspecInput::new("pubspec.yaml", "name: demo"),
        ],
    ));

    assert_eq!(analysis.files[0].path, "lib/a.dart");
    assert_eq!(analysis.files[1].path, "lib/z.dart");
    assert_eq!(analysis.pubspecs[0].path, "packages/z/pubspec.yaml");
    assert_eq!(analysis.pubspecs[1].path, "pubspec.yaml");
    assert!(analysis
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.path.is_some()));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.path.as_deref() == Some("lib/z.dart")));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.path.as_deref() == Some("packages/z/pubspec.yaml") }));
}

#[test]
fn analyzes_project_summary_from_files_and_pubspecs() {
    let dart = r#"
import 'package:flutter/widgets.dart';

class HomeScreen extends StatelessWidget {
}
"#;
    let pubspec = r#"
name: demo_app
dependencies:
  flutter:
    sdk: flutter
"#;

    let analysis = analyze_project(
        DartProjectInput::new(
            "D:\\apps\\demo_app",
            vec![DartFileInput::new("lib\\main.dart", dart)],
            vec![PubspecInput::new("pubspec.yaml", pubspec)],
        )
        .with_package_configs(vec![PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{"configVersion":2,"packages":[]}"#,
        )]),
    );

    assert_eq!(analysis.root, "D:/apps/demo_app");
    assert_eq!(analysis.summary.dart_files, 1);
    assert_eq!(analysis.summary.pubspecs, 1);
    assert_eq!(analysis.summary.package_configs, 1);
    assert_eq!(analysis.package_configs[0].config_version, Some(2));
    assert_eq!(analysis.summary.imports, 1);
    assert_eq!(analysis.summary.flutter_widgets, 1);
    assert_eq!(analysis.summary.package_dependencies, 1);
    assert!(analysis.diagnostics.is_empty());
}

#[test]
fn does_not_treat_indented_flutter_constructor_calls_as_declarations() {
    let source = r#"
import 'package:flutter/material.dart';

class HomeScreen extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Card(
      child: ListTile(
        title: Text('Home'),
      ),
    );
  }
}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/home.dart", source));
    let names: Vec<_> = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();

    assert!(names.contains(&"HomeScreen"));
    assert!(!names.contains(&"Card"));
    assert!(!names.contains(&"ListTile"));
    assert!(!names.contains(&"Text"));
}

#[test]
fn class_constructor_initializer_is_top_level_variable_not_function() {
    let source = r#"
const storefrontSurfaceRegistry = StorefrontSurfaceRegistry(
  generated.generatedMobileManifest,
);

class StorefrontSurfaceRegistry {
}
"#;

    let analysis = analyze_file(DartFileInput::new(
        "lib/registry/storefront_surface_registry.dart",
        source,
    ));

    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "storefrontSurfaceRegistry"
            && declaration.kind == DartDeclarationKind::Variable
    }));
    assert_eq!(
        analysis
            .declarations
            .iter()
            .filter(|declaration| declaration.name == "StorefrontSurfaceRegistry")
            .count(),
        1
    );
}

#[test]
fn treats_riverpod_consumer_widget_as_flutter_widget_hint() {
    let source = r#"
import 'package:flutter_riverpod/flutter_riverpod.dart';

class StorefrontHomePage extends ConsumerWidget {
}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/routes/home.dart", source));

    assert!(analysis.flutter.widgets.iter().any(|widget| {
        widget.class_name == "StorefrontHomePage" && widget.base_class == "ConsumerWidget"
    }));
    assert!(analysis.flutter.imports_flutter);
}

#[test]
fn extracts_direct_flutter_asset_and_localization_hints() {
    let source = r#"
import 'package:flutter/material.dart';
import 'package:flutter_gen/gen_l10n/app_localizations.dart';

Widget logo(BuildContext context) {
  return DecoratedBox(
    decoration: const BoxDecoration(
      image: DecorationImage(image: AssetImage('assets/brand/logo.png')),
    ),
    child: Text(AppLocalizations.of(context)!.welcomeMessage),
  );
}

Future<String> loadCopy(BuildContext context) {
  return DefaultAssetBundle.of(context).loadString('assets/copy/welcome.txt');
}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/widgets/logo.dart", source));

    assert!(analysis.flutter.assets.iter().any(|asset| {
        asset.path == "assets/brand/logo.png"
            && asset.source == dartscope_core::FlutterAssetSource::AssetImage
    }));
    assert!(analysis.flutter.assets.iter().any(|asset| {
        asset.path == "assets/copy/welcome.txt"
            && asset.source == dartscope_core::FlutterAssetSource::DefaultAssetBundleLoadString
    }));
    assert!(analysis
        .flutter
        .localizations
        .iter()
        .any(|localization| localization.key == "welcomeMessage"));
}

#[test]
fn extracts_go_route_hints_without_building_a_full_route_graph() {
    let source = r#"
import 'package:go_router/go_router.dart';

const homePath = '/';
const modulesRootPath = '/modules';
const String profilePath = '/profile';

GoRouter buildRouter() {
  return GoRouter(
    routes: [
      GoRoute(
        path: homePath,
        builder: (context, state) => const HomePage(),
      ),
      GoRoute(
        path: '$modulesRootPath/:routeSegment',
        name: 'modules:surface',
        builder: (context, state) => const ModulePage(),
      ),
      GoRoute(
        path: profilePath,
        builder: (context, state) => const ProfilePage(),
      ),
    ],
  );
}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/routes/app_router.dart", source));

    assert_eq!(analysis.flutter.routes.len(), 3);
    assert!(analysis
        .string_constants
        .iter()
        .any(|constant| { constant.name == "modulesRootPath" && constant.value == "/modules" }));
    assert!(analysis.flutter.routes.iter().any(|route| {
        route.path == "homePath"
            && route.path_kind == FlutterRoutePathKind::Expression
            && route.resolved_path.as_deref() == Some("/")
            && route.confidence == Confidence::High
    }));
    assert!(analysis.flutter.routes.iter().any(|route| {
        route.path == "$modulesRootPath/:routeSegment"
            && route.name.as_deref() == Some("modules:surface")
            && route.path_kind == FlutterRoutePathKind::Expression
            && route.resolved_path.as_deref() == Some("/modules/:routeSegment")
            && route.confidence == Confidence::High
    }));
    assert!(analysis.flutter.routes.iter().any(|route| {
        route.path == "profilePath"
            && route.resolved_path.as_deref() == Some("/profile")
            && route.confidence == Confidence::High
    }));
}

#[test]
fn extracts_material_app_routes_map_hints() {
    let source = r#"
import 'package:flutter/material.dart';

class App extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      routes: <String, WidgetBuilder>{
        // '/commented': (context) => const CommentedPage(),
        '/': (context) => const HomePage(),
        '/settings': (context) => const SettingsPage(),
      },
    );
  }
}
"#;

    let analysis = analyze_file(DartFileInput::new("lib/main.dart", source));

    assert_eq!(analysis.flutter.routes.len(), 2);
    assert!(!analysis
        .flutter
        .routes
        .iter()
        .any(|route| route.path == "/commented"));
    assert!(analysis.flutter.routes.iter().any(|route| {
        route.constructor == "MaterialApp.routes"
            && route.path == "/"
            && route.path_kind == FlutterRoutePathKind::Literal
            && route.resolved_path.as_deref() == Some("/")
            && route.confidence == Confidence::High
    }));
    assert!(analysis.flutter.routes.iter().any(|route| {
        route.constructor == "MaterialApp.routes"
            && route.path == "/settings"
            && route.resolved_path.as_deref() == Some("/settings")
    }));
}
