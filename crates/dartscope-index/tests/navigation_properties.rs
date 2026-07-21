use dartscope_core::{
    DartCompilationEnvironment, DartDeclarationKind, DartFileInput, DartProjectInput,
    DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartIndexOptions, DartWorkspaceIndex, DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

const TYPES: &str = r#"
class Service {
  static int count = 0;
  static int get status => count;
  static set status(int value) { count = value; }
  static int _hidden = 0;

  int value = 0;
  int get label => 'service';
  set label(String value) {}

  void exercise() {
    final first = this.value;
    this.value = 1;
    final second = this.label;
    this.label = 'updated';
    final absent = this.missing;
  }
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types;

void run() {
  final count = types.Service.count;
  types.Service.count = 1;
  final status = types.Service.status;
  types.Service.status = 2;
  final hidden = types.Service._hidden;
  final absent = types.Service.missing;
}
"#;

#[test]
fn resolves_exact_field_getter_and_setter_targets() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let count_read = occurrence(CLIENT, "Service.count;", "count");
    let count_write = occurrence(CLIENT, "Service.count =", "count");
    let status_read = occurrence(CLIENT, "Service.status;", "status");
    let status_write = occurrence(CLIENT, "Service.status =", "status");
    let hidden = occurrence(CLIENT, "Service._hidden", "_hidden");
    let missing = occurrence(CLIENT, "Service.missing", "missing");
    let value_read = occurrence(TYPES, "this.value;", "value");
    let value_write = occurrence(TYPES, "this.value =", "value");
    let label_read = occurrence(TYPES, "this.label;", "label");
    let label_write = occurrence(TYPES, "this.label =", "label");
    let local_missing = occurrence(TYPES, "this.missing", "missing");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/client.dart", count_read),
        DartDefinitionQuery::new("lib/client.dart", count_write),
        DartDefinitionQuery::new("lib/client.dart", status_read),
        DartDefinitionQuery::new("lib/client.dart", status_write),
        DartDefinitionQuery::new("lib/client.dart", hidden),
        DartDefinitionQuery::new("lib/client.dart", missing),
        DartDefinitionQuery::new("lib/types.dart", value_read),
        DartDefinitionQuery::new("lib/types.dart", value_write),
        DartDefinitionQuery::new("lib/types.dart", label_read),
        DartDefinitionQuery::new("lib/types.dart", label_write),
        DartDefinitionQuery::new("lib/types.dart", local_missing),
    ]);

    assert_property_target(
        resolution_at(&batch.resolutions, "lib/client.dart", count_read),
        DartDefinitionResolutionStatus::Resolved,
        "count",
        DartDeclarationKind::Field,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/client.dart", count_write),
        DartDefinitionResolutionStatus::Resolved,
        "count",
        DartDeclarationKind::Field,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/client.dart", status_read),
        DartDefinitionResolutionStatus::Resolved,
        "status",
        DartDeclarationKind::Getter,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/client.dart", status_write),
        DartDefinitionResolutionStatus::Resolved,
        "status",
        DartDeclarationKind::Setter,
        "lib/types.dart",
    );
    let hidden_resolution = resolution_at(&batch.resolutions, "lib/client.dart", hidden);
    assert_property_target(
        hidden_resolution,
        DartDefinitionResolutionStatus::NotVisible,
        "_hidden",
        DartDeclarationKind::Field,
        "lib/types.dart",
    );
    assert!(matches!(
        &hidden_resolution.targets[0],
        DartDefinitionTarget::Namespace(candidate)
            if candidate.basis == DartSymbolResolutionBasis::NotVisible
    ));
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/client.dart", missing),
        "Service",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/types.dart", value_read),
        DartDefinitionResolutionStatus::Resolved,
        "value",
        DartDeclarationKind::Field,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/types.dart", value_write),
        DartDefinitionResolutionStatus::Resolved,
        "value",
        DartDeclarationKind::Field,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/types.dart", label_read),
        DartDefinitionResolutionStatus::Resolved,
        "label",
        DartDeclarationKind::Getter,
        "lib/types.dart",
    );
    assert_property_target(
        resolution_at(&batch.resolutions, "lib/types.dart", label_write),
        DartDefinitionResolutionStatus::Resolved,
        "label",
        DartDeclarationKind::Setter,
        "lib/types.dart",
    );
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/types.dart", local_missing),
        "Service",
    );

    let count_target =
        resolution_at(&batch.resolutions, "lib/client.dart", count_read).targets[0].clone();
    let references = context.find_references(std::slice::from_ref(&count_target));
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].target, count_target);
    assert_eq!(references.results[0].references.len(), 2);
    assert!(references.results[0]
        .references
        .iter()
        .all(|reference| reference.span.byte_start == count_read || reference.span.byte_start == count_write));
}

const FIRST: &str = r#"
class Shared {
  static int value = 1;
}
"#;
const SECOND: &str = r#"
class Shared {
  static int value = 2;
}
"#;
const SERVICE_STUB: &str = r#"
class ConditionalService {
  static int get value => 0;
}
"#;
const SERVICE_IO: &str = r#"
class ConditionalService {
  static int get value => 1;
}
"#;
const EVIDENCE_CLIENT: &str = r#"
import 'first.dart';
import 'second.dart';
import 'service_stub.dart' if (dart.library.io) 'service_io.dart';
import 'package:widgets/api.dart' as widgets;

void run() {
  final shared = Shared.value;
  final conditional = ConditionalService.value;
  final external = widgets.Widget.value;
}
"#;

#[test]
fn preserves_ambiguous_conditional_and_external_property_evidence() {
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
    let shared = occurrence(EVIDENCE_CLIENT, "Shared.value", "value");
    let conditional = occurrence(EVIDENCE_CLIENT, "ConditionalService.value", "value");
    let external = occurrence(EVIDENCE_CLIENT, "Widget.value", "value");
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
            if candidate.kind == DartDeclarationKind::Field && candidate.name == "value"
    )));

    let conditional = resolution_at(&unresolved.resolutions, "lib/client.dart", conditional);
    assert_eq!(
        conditional.status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(conditional.targets.len(), 2);
    assert!(conditional.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Getter && candidate.name == "value"
    )));

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
    let resolved = DartWorkspaceResolutionContext::with_options(&analysis, &options)
        .find_definitions(&[DartDefinitionQuery::new(
            "lib/client.dart",
            conditional.query.byte_offset,
        )]);
    assert_property_target(
        &resolved.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "value",
        DartDeclarationKind::Getter,
        "lib/service_io.dart",
    );
}

const OWNER: &str = r#"
library sample;
part 'part.dart';

void run() {
  final value = PartService.value;
}
"#;
const PART: &str = r#"
part of 'owner.dart';

class PartService {
  static int value = 1;
}
"#;

#[test]
fn resolves_part_properties_and_preserves_snapshot_parity() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let value = occurrence(OWNER, "PartService.value", "value");
    let query = DartDefinitionQuery::new("lib/owner.dart", value);
    let expected = DartWorkspaceResolutionContext::new(&analysis)
        .find_definitions(std::slice::from_ref(&query));
    assert_property_target(
        &expected.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "value",
        DartDeclarationKind::Field,
        "lib/part.dart",
    );

    let index = DartWorkspaceIndex::from_reference_project(analysis);
    let snapshot = index.snapshot();
    let actual = DartWorkspaceResolutionContext::from_snapshot(snapshot.as_ref())
        .find_definitions(&[query]);
    assert_eq!(actual, expected);
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

fn assert_property_target(
    resolution: &DartDefinitionResolution,
    status: DartDefinitionResolutionStatus,
    name: &str,
    kind: DartDeclarationKind,
    path: &str,
) {
    assert_eq!(resolution.status, status);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, kind);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, path);
        }
        target => panic!("unexpected property target: {target:?}"),
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
        target => panic!("unexpected owner fallback: {target:?}"),
    }
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
