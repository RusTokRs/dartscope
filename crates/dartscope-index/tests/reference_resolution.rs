use dartscope_core::{
    DartCompilationEnvironment, DartFileInput, DartProjectInput, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus,
};
use dartscope_index::{
    DartIndexOptions, resolve_project_identifier_references,
    resolve_project_identifier_references_with_options,
};
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_parser_references_in_one_deterministic_batch() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/api.dart", "class ApiType {}\n"),
            DartFileInput::new("lib/first.dart", "class Shared {}\n"),
            DartFileInput::new("lib/second.dart", "class Shared {}\n"),
            DartFileInput::new(
                "lib/client.dart",
                r#"
import 'api.dart' as api;
import 'first.dart';
import 'second.dart';

class LocalType {}

void run() {
  api.ApiType();
  LocalType();
  Shared();
  MissingType();
}
"#,
            ),
        ],
        vec![],
    ));

    let resolved = resolve_project_identifier_references(&analysis);
    assert_eq!(resolved.resolutions.len(), 4);

    let api = &resolved.resolutions[0];
    assert_eq!(api.reference.name, "ApiType");
    assert_eq!(api.reference.prefix.as_deref(), Some("api"));
    assert_eq!(api.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(
        api.candidates[0].basis,
        DartSymbolResolutionBasis::DirectImport
    );

    let local = &resolved.resolutions[1];
    assert_eq!(local.reference.name, "LocalType");
    assert_eq!(local.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(
        local.candidates[0].basis,
        DartSymbolResolutionBasis::SameFile
    );

    let shared = &resolved.resolutions[2];
    assert_eq!(shared.reference.name, "Shared");
    assert_eq!(shared.status, DartSymbolResolutionStatus::Ambiguous);
    assert_eq!(shared.candidates.len(), 2);

    let missing = &resolved.resolutions[3];
    assert_eq!(missing.reference.name, "MissingType");
    assert_eq!(missing.status, DartSymbolResolutionStatus::Missing);
    assert!(missing.candidates.is_empty());
}

#[test]
fn batch_references_share_conditional_namespace_semantics() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/service_stub.dart", "class PlatformService {}\n"),
            DartFileInput::new("lib/service_io.dart", "class PlatformService {}\n"),
            DartFileInput::new(
                "lib/client.dart",
                "import 'service_stub.dart' if (dart.library.io) 'service_io.dart';\nvoid run() { PlatformService(); }\n",
            ),
        ],
        vec![],
    ));

    let unresolved = resolve_project_identifier_references(&analysis);
    assert_eq!(unresolved.resolutions.len(), 1);
    assert_eq!(
        unresolved.resolutions[0].status,
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(unresolved.resolutions[0].candidates.len(), 2);

    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );
    let resolved = resolve_project_identifier_references_with_options(&analysis, &options);
    assert_eq!(
        resolved.resolutions[0].status,
        DartSymbolResolutionStatus::Resolved
    );
    assert_eq!(
        resolved.resolutions[0].candidates[0].declaration_path,
        "lib/service_io.dart"
    );
}
