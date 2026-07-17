use dartscope_core::{DartFileInput, DartProjectInput, PubspecInput};
use dartscope_flutter::{
    FlutterEcosystemConvention, FlutterEcosystemConventionStatus, FlutterEcosystemFindingKind,
    FlutterEcosystemSupportTableVersion, analyze_flutter_ecosystem,
    flutter_ecosystem_support_table,
};
use dartscope_parse::analyze_project;

#[test]
fn support_table_v1_is_deterministic_and_versioned() {
    let table = flutter_ecosystem_support_table();

    assert_eq!(table.version, FlutterEcosystemSupportTableVersion::V1);
    assert_eq!(table.version.version(), 1);
    assert_eq!(
        table
            .entries
            .iter()
            .map(|entry| entry.package.as_str())
            .collect::<Vec<_>>(),
        ["go_router", "provider", "flutter_riverpod", "flutter_bloc"]
    );
    assert_eq!(table.entries[0].supported_range, ">=14.0.0 <18.0.0");
    assert_eq!(table.entries[0].fixture_version, "17.3.0");
    assert!(
        table.entries[1]
            .patterns
            .contains(&"BuildContext.watch".to_string())
    );
}

#[test]
fn enabled_supported_packages_emit_evidence_backed_findings() {
    let source = r#"import 'package:go_router/go_router.dart' as routing;
import 'package:provider/provider.dart' as provider;
import 'package:flutter_riverpod/flutter_riverpod.dart' as riverpod;
import 'package:flutter_bloc/flutter_bloc.dart' as bloc;

final router = routing.GoRouter(
  routes: [routing.GoRoute(path: '/home', builder: homeBuilder)],
);

Widget buildRoot(BuildContext context) => provider.MultiProvider(
  providers: [provider.Provider(create: createModel)],
  child: riverpod.ProviderScope(
    child: bloc.BlocProvider(create: createBloc, child: const Screen()),
  ),
);

final watched = context.watch<Model>();

class Screen extends riverpod.ConsumerWidget {
  const Screen({super.key});
}
"#;
    let pubspec = r#"name: demo
dependencies:
  go_router: ^17.3.0
  provider: ^6.1.5
  flutter_riverpod: ^3.3.0
  flutter_bloc: ^9.1.0
"#;
    let project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new("lib/app.dart", source)],
        vec![PubspecInput::new("pubspec.yaml", pubspec)],
    ));
    let analysis = analyze_flutter_ecosystem(
        &project,
        &[
            FlutterEcosystemConvention::FlutterBloc,
            FlutterEcosystemConvention::Provider,
            FlutterEcosystemConvention::GoRouter,
            FlutterEcosystemConvention::FlutterRiverpod,
        ],
    );

    assert_eq!(analysis.support_table_version.version(), 1);
    assert_eq!(analysis.conventions.len(), 4);
    assert!(
        analysis
            .conventions
            .iter()
            .all(|entry| entry.status == FlutterEcosystemConventionStatus::Active)
    );
    assert!(analysis.conventions.iter().all(|entry| {
        entry.package_evidence.len() == 1
            && entry.package_evidence[0].pubspec_path == "pubspec.yaml"
            && entry.package_evidence[0].version_constraint.is_some()
    }));

    let go_router = convention(&analysis, FlutterEcosystemConvention::GoRouter);
    assert!(
        go_router
            .findings
            .iter()
            .any(|finding| finding.pattern == "GoRouter")
    );
    assert!(
        go_router
            .findings
            .iter()
            .any(|finding| finding.pattern == "GoRoute")
    );

    let provider = convention(&analysis, FlutterEcosystemConvention::Provider);
    assert!(
        provider
            .findings
            .iter()
            .any(|finding| finding.pattern == "MultiProvider")
    );
    assert!(
        provider
            .findings
            .iter()
            .any(|finding| finding.pattern == "Provider")
    );
    assert!(
        provider
            .findings
            .iter()
            .any(|finding| finding.pattern == "BuildContext.watch")
    );

    let riverpod = convention(&analysis, FlutterEcosystemConvention::FlutterRiverpod);
    assert!(
        riverpod
            .findings
            .iter()
            .any(|finding| finding.pattern == "ProviderScope")
    );
    assert!(riverpod.findings.iter().any(|finding| {
        finding.pattern == "ConsumerWidget"
            && finding.kind == FlutterEcosystemFindingKind::BaseClass
    }));

    let bloc = convention(&analysis, FlutterEcosystemConvention::FlutterBloc);
    assert!(
        bloc.findings
            .iter()
            .any(|finding| finding.pattern == "BlocProvider")
    );
    assert!(
        analysis
            .conventions
            .iter()
            .flat_map(|entry| &entry.findings)
            .all(|finding| finding.file_path == "lib/app.dart"
                && finding.span.byte_end > finding.span.byte_start)
    );
}

#[test]
fn activation_is_strictly_opt_in() {
    let project = supported_go_router_project();

    let disabled = analyze_flutter_ecosystem(&project, &[]);
    assert!(disabled.conventions.is_empty());

    let enabled = analyze_flutter_ecosystem(&project, &[FlutterEcosystemConvention::GoRouter]);
    assert_eq!(enabled.conventions.len(), 1);
    assert_eq!(
        enabled.conventions[0].status,
        FlutterEcosystemConventionStatus::Active
    );
    assert!(!enabled.conventions[0].findings.is_empty());
}

#[test]
fn unsupported_missing_and_unverifiable_dependencies_do_not_activate_semantics() {
    let source = r#"import 'package:go_router/go_router.dart';
import 'package:provider/provider.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

final route = GoRoute(path: '/future', builder: builder);
final providers = MultiProvider(providers: [], child: child);
final bloc = BlocProvider(create: createBloc, child: child);
"#;
    let pubspec = r#"name: demo
dependencies:
  go_router: ^18.0.0
  provider:
    path: ../provider
"#;
    let project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new("lib/app.dart", source)],
        vec![PubspecInput::new("pubspec.yaml", pubspec)],
    ));
    let analysis = analyze_flutter_ecosystem(
        &project,
        &[
            FlutterEcosystemConvention::GoRouter,
            FlutterEcosystemConvention::Provider,
            FlutterEcosystemConvention::FlutterBloc,
        ],
    );

    assert_eq!(
        convention(&analysis, FlutterEcosystemConvention::GoRouter).status,
        FlutterEcosystemConventionStatus::UnsupportedVersion
    );
    assert_eq!(
        convention(&analysis, FlutterEcosystemConvention::Provider).status,
        FlutterEcosystemConventionStatus::UnverifiableVersion
    );
    assert_eq!(
        convention(&analysis, FlutterEcosystemConvention::FlutterBloc).status,
        FlutterEcosystemConventionStatus::DependencyMissing
    );
    assert!(
        analysis
            .conventions
            .iter()
            .all(|entry| entry.findings.is_empty())
    );
}

#[test]
fn exact_package_import_is_required_for_findings() {
    let source = r#"void build() {
  Provider(create: createModel, child: child);
  BlocProvider(create: createBloc, child: child);
}
"#;
    let pubspec = r#"name: demo
dependencies:
  provider: ^6.1.5
  flutter_bloc: ^9.1.1
"#;
    let project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new("lib/plain.dart", source)],
        vec![PubspecInput::new("pubspec.yaml", pubspec)],
    ));
    let analysis = analyze_flutter_ecosystem(
        &project,
        &[
            FlutterEcosystemConvention::Provider,
            FlutterEcosystemConvention::FlutterBloc,
        ],
    );

    assert!(analysis.conventions.iter().all(|entry| {
        entry.status == FlutterEcosystemConventionStatus::Active && entry.findings.is_empty()
    }));
}

fn supported_go_router_project() -> dartscope_core::DartProjectAnalysis {
    analyze_project(DartProjectInput::new(
        "fixture",
        vec![DartFileInput::new(
            "lib/router.dart",
            "import 'package:go_router/go_router.dart';\nfinal route = GoRoute(path: '/');\n",
        )],
        vec![PubspecInput::new(
            "pubspec.yaml",
            "name: demo\ndependencies:\n  go_router: '>=16.0.0 <18.0.0'\n",
        )],
    ))
}

fn convention(
    analysis: &dartscope_flutter::FlutterEcosystemAnalysis,
    convention: FlutterEcosystemConvention,
) -> &dartscope_flutter::FlutterEcosystemConventionAnalysis {
    analysis
        .conventions
        .iter()
        .find(|entry| entry.convention == convention)
        .expect("enabled convention")
}
