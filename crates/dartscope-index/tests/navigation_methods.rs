use dartscope_core::{
    DartCompilationEnvironment, DartDeclarationKind, DartFileInput, DartProjectInput,
    DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartIndexOptions, DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

const TYPES: &str = r#"
class Service {
  static void build() {}
  static void _hidden() {}

  void exercise() {
    this.work();
    this._private();
    this.missing();
  }

  void work() {}
  void _private() {}
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types;

void run() {
  types.Service.build();
  types.Service._hidden();
  types.Service.missing();
}
"#;

#[test]
fn resolves_exact_static_and_this_method_targets() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let build = occurrence(CLIENT, "Service.build", "build");
    let hidden = occurrence(CLIENT, "Service._hidden", "_hidden");
    let missing = occurrence(CLIENT, "Service.missing", "missing");
    let work = occurrence(TYPES, "this.work", "work");
    let private = occurrence(TYPES, "this._private", "_private");
    let local_missing = occurrence(TYPES, "this.missing", "missing");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/client.dart", build),
        DartDefinitionQuery::new("lib/client.dart", hidden),
        DartDefinitionQuery::new("lib/client.dart", missing),
        DartDefinitionQuery::new("lib/types.dart", work),
        DartDefinitionQuery::new("lib/types.dart", private),
        DartDefinitionQuery::new("lib/types.dart", local_missing),
    ]);

    assert_method_target(
        resolution_at(&batch.resolutions, "lib/client.dart", build),
        DartDefinitionResolutionStatus::Resolved,
        "build",
        "lib/types.dart",
    );
    let hidden = resolution_at(&batch.resolutions, "lib/client.dart", hidden);
    assert_method_target(
        hidden,
        DartDefinitionResolutionStatus::NotVisible,
        "_hidden",
        "lib/types.dart",
    );
    assert!(matches!(
        &hidden.targets[0],
        DartDefinitionTarget::Namespace(candidate)
            if candidate.basis == DartSymbolResolutionBasis::NotVisible
    ));
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/client.dart", missing),
        "Service",
    );
    assert_method_target(
        resolution_at(&batch.resolutions, "lib/types.dart", work),
        DartDefinitionResolutionStatus::Resolved,
        "work",
        "lib/types.dart",
    );
    assert_method_target(
        resolution_at(&batch.resolutions, "lib/types.dart", private),
        DartDefinitionResolutionStatus::Resolved,
        "_private",
        "lib/types.dart",
    );
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/types.dart", local_missing),
        "Service",
    );

    let build_target =
        resolution_at(&batch.resolutions, "lib/client.dart", build).targets[0].clone();
    let references = context.find_references(&[build_target.clone()]);
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].target, build_target);
    assert_eq!(references.results[0].references.len(), 1);
    assert_eq!(references.results[0].references[0].span.byte_start, build);
}

const FIRST: &str = r#"
class Shared {
  static void open() {}
}
"#;
const SECOND: &str = r#"
class Shared {
  static void open() {}
}
"#;
const SERVICE_STUB: &str = r#"
class ConditionalService {
  static void open() {}
}
"#;
const SERVICE_IO: &str = r#"
class ConditionalService {
  static void open() {}
}
"#;
const EVIDENCE_CLIENT: &str = r#"
import 'first.dart';
import 'second.dart';
import 'service_stub.dart' if (dart.library.io) 'service_io.dart';
import 'package:widgets/api.dart' as widgets;

void run() {
  Shared.open();
  ConditionalService.open();
  widgets.Widget.open();
}
"#;

#[test]
fn preserves_ambiguous_conditional_and_external_method_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/first.dart", FIRST),
            DartFileInput::new("lib/second.dart", SECOND),
            DartFileInput::new("lib/service_stub.dart", SERVICE_STUB),
            DartFileInput::new("lib/service_io.dart", SERVICE_IO),
            DartFileInput::new("lib/client.dart", EVIDENCE_CLIENT),
        ],
        vec![],
    ));
    let shared = occurrence(EVIDENCE_CLIENT, "Shared.open", "open");
    let conditional = occurrence(EVIDENCE_CLIENT, "ConditionalService.open", "open");
    let external = occurrence(EVIDENCE_CLIENT, "Widget.open", "open");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", shared),
        DartDefinitionQuery::new("lib/client.dart", conditional),
        DartDefinitionQuery::new("lib/client.dart", external),
    ];
    let unresolved = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&queries);

    let shared = resolution_at(&unresolved.resolutions, "lib/client.dart", shared);
    assert_eq!(shared.status, DartDefinitionResolutionStatus::Ambiguous);
    assert_eq!(shared.targets.len(), 2);
    assert!(shared.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Method && candidate.name == "open"
    )));

    let conditional = resolution_at(&unresolved.resolutions, "lib/client.dart", conditional);
    assert_eq!(
        conditional.status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(conditional.targets.len(), 2);

    let external = resolution_at(&unresolved.resolutions, "lib/client.dart", external);
    assert_eq!(
        external.status,
        DartDefinitionResolutionStatus::ExternalUnindexed
    );
    assert_eq!(external.external_uris, ["package:widgets/api.dart"]);
    assert!(external.targets.is_empty());

    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );
    let resolved =
        DartWorkspaceResolutionContext::with_options(&analysis, &options).find_definitions(&[
            DartDefinitionQuery::new("lib/client.dart", conditional.query.byte_offset),
        ]);
    assert_method_target(
        &resolved.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "open",
        "lib/service_io.dart",
    );
}

const OWNER: &str = r#"
library sample;
part 'part.dart';

void run() {
  PartService.open();
}
"#;
const PART: &str = r#"
part of 'owner.dart';

class PartService {
  static void open() {}
}
"#;

#[test]
fn resolves_static_methods_declared_in_a_part_library() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let open = occurrence(OWNER, "PartService.open", "open");
    let batch = DartWorkspaceResolutionContext::new(&analysis)
        .find_definitions(&[DartDefinitionQuery::new("lib/owner.dart", open)]);
    assert_method_target(
        &batch.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "open",
        "lib/part.dart",
    );
}

fn resolution_at<'a>(
    resolutions: &'a [DartDefinitionResolution],
    path: &str,
    byte_offset: usize,
) -> &'a DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| {
            resolution.query.source_path == path && resolution.query.byte_offset == byte_offset
        })
        .unwrap_or_else(|| panic!("missing definition result at {path}:{byte_offset}"))
}

fn assert_method_target(
    resolution: &DartDefinitionResolution,
    status: DartDefinitionResolutionStatus,
    name: &str,
    path: &str,
) {
    assert_eq!(resolution.status, status);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, DartDeclarationKind::Method);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, path);
        }
        target => panic!("unexpected method target: {target:?}"),
    }
}

fn assert_owner_fallback(resolution: &DartDefinitionResolution, owner: &str) {
    assert_eq!(resolution.status, DartDefinitionResolutionStatus::Missing);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.name, owner);
            assert_eq!(candidate.kind, DartDeclarationKind::Class);
        }
        target => panic!("unexpected owner target: {target:?}"),
    }
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
