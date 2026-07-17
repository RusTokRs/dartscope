use crate::*;
use dartscope_core::*;
use dartscope_parse::analyze_project;

#[test]
fn resolves_same_file_and_part_library_declarations() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/owner.dart",
                "library app;\npart 'src/member.dart';\nclass OwnerType {}\n",
            ),
            DartFileInput::new(
                "lib/src/member.dart",
                "part of '../owner.dart';\nclass PartType {}\n",
            ),
        ],
        vec![],
    ));

    let same_file = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/owner.dart", "OwnerType"),
    );
    assert_eq!(same_file.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(same_file.candidates.len(), 1);
    assert_eq!(
        same_file.candidates[0].basis,
        DartSymbolResolutionBasis::SameFile
    );

    let same_library = resolve_symbol(&project, DartSymbolQuery::new("lib/owner.dart", "PartType"));
    assert_eq!(same_library.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(same_library.candidates.len(), 1);
    assert_eq!(
        same_library.candidates[0].basis,
        DartSymbolResolutionBasis::SameLibrary
    );
    assert_eq!(
        same_library.candidates[0].declaration_path,
        "lib/src/member.dart"
    );
}

#[test]
fn respects_prefixed_imports_combinators_and_private_names() {
    let declarations = "class PublicType {}\nclass HiddenType {}\nclass _PrivateType {}\n";
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/declarations.dart", declarations),
            DartFileInput::new(
                "lib/prefixed.dart",
                "import 'declarations.dart' as api show PublicType, _PrivateType;\n",
            ),
            DartFileInput::new(
                "lib/hidden.dart",
                "import 'declarations.dart' hide HiddenType;\n",
            ),
        ],
        vec![],
    ));

    let prefixed = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/prefixed.dart", "PublicType").with_prefix("api"),
    );
    assert_eq!(prefixed.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(
        prefixed.candidates[0].basis,
        DartSymbolResolutionBasis::DirectImport
    );

    let unqualified = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/prefixed.dart", "PublicType"),
    );
    assert_eq!(unqualified.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(
        unqualified.candidates[0].basis,
        DartSymbolResolutionBasis::NotVisible
    );

    let hidden = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/hidden.dart", "HiddenType"),
    );
    assert_eq!(hidden.status, DartSymbolResolutionStatus::NotVisible);

    let private = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/prefixed.dart", "_PrivateType").with_prefix("api"),
    );
    assert_eq!(private.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(private.candidates.len(), 1);
}

#[test]
fn resolves_re_exports_and_preserves_ambiguous_candidates() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/src/first.dart", "class Shared {}\n"),
            DartFileInput::new("lib/src/second.dart", "class Shared {}\n"),
            DartFileInput::new(
                "lib/api.dart",
                "export 'src/first.dart';\nexport 'src/second.dart';\n",
            ),
            DartFileInput::new("lib/client.dart", "import 'api.dart';\n"),
        ],
        vec![],
    ));

    let resolution = resolve_symbol(&project, DartSymbolQuery::new("lib/client.dart", "Shared"));
    assert_eq!(resolution.status, DartSymbolResolutionStatus::Ambiguous);
    assert_eq!(resolution.candidates.len(), 2);
    assert_eq!(
        resolution
            .candidates
            .iter()
            .map(|candidate| candidate.declaration_path.as_str())
            .collect::<Vec<_>>(),
        ["lib/src/first.dart", "lib/src/second.dart"]
    );
    assert!(
        resolution
            .candidates
            .iter()
            .all(|candidate| candidate.basis == DartSymbolResolutionBasis::ReExport)
    );
}

#[test]
fn conditional_imports_require_an_environment_before_resolution() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/service_stub.dart", "class PlatformService {}\n"),
            DartFileInput::new("lib/service_io.dart", "class PlatformService {}\n"),
            DartFileInput::new(
                "lib/client.dart",
                "import 'service_stub.dart' if (dart.library.io) 'service_io.dart';\n",
            ),
        ],
        vec![],
    ));

    let unresolved = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/client.dart", "PlatformService"),
    );
    assert_eq!(
        unresolved.status,
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(unresolved.candidates.len(), 2);

    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );
    let resolved = resolve_symbol_with_options(
        &project,
        DartSymbolQuery::new("lib/client.dart", "PlatformService"),
        &options,
    );
    assert_eq!(resolved.status, DartSymbolResolutionStatus::Resolved);
    assert_eq!(resolved.candidates.len(), 1);
    assert_eq!(
        resolved.candidates[0].declaration_path,
        "lib/service_io.dart"
    );
    assert_eq!(
        resolved.candidates[0].basis,
        DartSymbolResolutionBasis::DirectImport
    );
}

#[test]
fn distinguishes_missing_not_visible_and_missing_source_files() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/declared.dart", "class Existing {}\n"),
            DartFileInput::new("lib/client.dart", "void run() {}\n"),
        ],
        vec![],
    ));

    let not_visible = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/client.dart", "Existing"),
    );
    assert_eq!(not_visible.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(not_visible.candidates.len(), 1);

    let missing = resolve_symbol(&project, DartSymbolQuery::new("lib/client.dart", "Absent"));
    assert_eq!(missing.status, DartSymbolResolutionStatus::Missing);
    assert!(missing.candidates.is_empty());

    let missing_source = resolve_symbol(
        &project,
        DartSymbolQuery::new("lib/missing.dart", "Existing"),
    );
    assert_eq!(
        missing_source.status,
        DartSymbolResolutionStatus::SourceFileMissing
    );
    assert!(missing_source.candidates.is_empty());
}
