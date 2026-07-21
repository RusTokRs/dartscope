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
class Imported {
  Imported();
  Imported.named();
  Imported._hidden();
}

class Implicit {}

class NamedOnly {
  NamedOnly.named();
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types show Imported, Implicit, NamedOnly;

void run() {
  new types.Imported();
  new types.Imported.named();
  new types.Imported._hidden();
  new types.Imported.missing();
  new types.Implicit();
  new types.NamedOnly();
}
"#;

#[test]
fn resolves_exact_prefixed_constructors_and_preserves_owner_fallback_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let offsets = CLIENT
        .match_indices("Imported")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let implicit = occurrence(CLIENT, "types.Implicit", "Implicit");
    let named_only = occurrence(CLIENT, "types.NamedOnly", "NamedOnly");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", offsets[1]),
        DartDefinitionQuery::new("lib/client.dart", offsets[2]),
        DartDefinitionQuery::new("lib/client.dart", offsets[3]),
        DartDefinitionQuery::new("lib/client.dart", offsets[4]),
        DartDefinitionQuery::new("lib/client.dart", implicit),
        DartDefinitionQuery::new("lib/client.dart", named_only),
    ];
    let batch = context.find_definitions(&queries);

    let unnamed = resolution_at(&batch.resolutions, offsets[1]);
    assert_eq!(unnamed.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(unnamed, "Imported", "lib/types.dart");

    let named = resolution_at(&batch.resolutions, offsets[2]);
    assert_eq!(named.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(named, "Imported.named", "lib/types.dart");

    let hidden = resolution_at(&batch.resolutions, offsets[3]);
    assert_eq!(hidden.status, DartDefinitionResolutionStatus::NotVisible);
    assert_constructor_target(hidden, "Imported._hidden", "lib/types.dart");
    match &hidden.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.basis, DartSymbolResolutionBasis::NotVisible);
        }
        target => panic!("unexpected hidden target: {target:?}"),
    }

    let missing = resolution_at(&batch.resolutions, offsets[4]);
    assert_eq!(missing.status, DartDefinitionResolutionStatus::Missing);
    assert_owner_target(missing, "Imported", DartDeclarationKind::Class);

    let implicit = resolution_at(&batch.resolutions, implicit);
    assert_eq!(implicit.status, DartDefinitionResolutionStatus::Resolved);
    assert_owner_target(implicit, "Implicit", DartDeclarationKind::Class);

    let named_only = resolution_at(&batch.resolutions, named_only);
    assert_eq!(named_only.status, DartDefinitionResolutionStatus::Missing);
    assert_owner_target(named_only, "NamedOnly", DartDeclarationKind::Class);

    let named_target = named.targets[0].clone();
    let references = context.find_references(std::slice::from_ref(&named_target));
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].target, named_target);
    assert_eq!(references.results[0].references.len(), 1);
    assert_eq!(
        references.results[0].references[0].span.byte_start,
        offsets[2]
    );
}

const FIRST: &str = r#"
class Shared {
  Shared.named();
}
"#;
const SECOND: &str = r#"
class Shared {
  Shared.named();
}
"#;
const SERVICE_STUB: &str = r#"
class Service {
  Service.named();
}
"#;
const SERVICE_IO: &str = r#"
class Service {
  Service.named();
}
"#;
const ENV_CLIENT: &str = r#"
import 'first.dart';
import 'second.dart';
import 'service_stub.dart' if (dart.library.io) 'service_io.dart';
import 'package:widgets/api.dart';

void run() {
  new Shared.named();
  new Service.named();
  new Widget.named();
}
"#;

#[test]
fn preserves_ambiguity_conditional_and_external_constructor_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/first.dart", FIRST),
            DartFileInput::new("lib/second.dart", SECOND),
            DartFileInput::new("lib/service_stub.dart", SERVICE_STUB),
            DartFileInput::new("lib/service_io.dart", SERVICE_IO),
            DartFileInput::new("lib/client.dart", ENV_CLIENT),
        ],
        vec![],
    ));
    let shared = occurrence(ENV_CLIENT, "Shared.named", "Shared");
    let service = occurrence(ENV_CLIENT, "Service.named", "Service");
    let widget = occurrence(ENV_CLIENT, "Widget.named", "Widget");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", shared),
        DartDefinitionQuery::new("lib/client.dart", service),
        DartDefinitionQuery::new("lib/client.dart", widget),
    ];
    let unresolved = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&queries);

    let shared = resolution_at(&unresolved.resolutions, shared);
    assert_eq!(shared.status, DartDefinitionResolutionStatus::Ambiguous);
    assert_eq!(shared.targets.len(), 2);
    assert!(shared.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Constructor
                && candidate.name == "Shared.named"
    )));

    let conditional = resolution_at(&unresolved.resolutions, service);
    assert_eq!(
        conditional.status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(conditional.targets.len(), 2);
    assert!(conditional.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Constructor
                && candidate.name == "Service.named"
    )));

    let external = resolution_at(&unresolved.resolutions, widget);
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
        .find_definitions(&[DartDefinitionQuery::new("lib/client.dart", service)]);
    let service = resolution_at(&resolved.resolutions, service);
    assert_eq!(service.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(service, "Service.named", "lib/service_io.dart");
}

const OWNER: &str = r#"
library sample;
part 'part.dart';

void run() {
  new PartType.named();
  new PartType._hidden();
}
"#;
const PART: &str = r#"
part of 'owner.dart';

class PartType {
  PartType.named();
  PartType._hidden();
}
"#;

#[test]
fn resolves_public_and_private_constructors_across_one_part_library() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let named = occurrence(OWNER, "PartType.named", "PartType");
    let hidden = occurrence(OWNER, "PartType._hidden", "PartType");
    let batch = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&[
        DartDefinitionQuery::new("lib/owner.dart", named),
        DartDefinitionQuery::new("lib/owner.dart", hidden),
    ]);

    let named = resolution_at(&batch.resolutions, named);
    assert_eq!(named.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(named, "PartType.named", "lib/part.dart");

    let hidden = resolution_at(&batch.resolutions, hidden);
    assert_eq!(hidden.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(hidden, "PartType._hidden", "lib/part.dart");
}

fn resolution_at(
    resolutions: &[DartDefinitionResolution],
    byte_offset: usize,
) -> &DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == byte_offset)
        .unwrap_or_else(|| panic!("missing definition result at {byte_offset}"))
}

fn assert_constructor_target(resolution: &DartDefinitionResolution, name: &str, path: &str) {
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, DartDeclarationKind::Constructor);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, path);
        }
        target => panic!("unexpected constructor target: {target:?}"),
    }
}

fn assert_owner_target(
    resolution: &DartDefinitionResolution,
    name: &str,
    kind: DartDeclarationKind,
) {
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, kind);
            assert_eq!(candidate.name, name);
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
