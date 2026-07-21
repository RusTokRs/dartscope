use dartscope_core::{DartDeclarationKind, DartFileInput, DartProjectInput};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

const TYPES: &str = r#"
enum State {
  idle;

  static void reset() {}
  static int get code => 1;
}

extension Extras on String {
  static void help() {}
  static int get value => 1;
}

class Factory {
  Factory();
  Factory.named();

  static void build() {}
  void work() {}

  void inspect() {
    final tearOff = this.work;
  }
}

class _Private {
  static void run() {}
  static int value = 1;
}

void local() {
  _Private.run();
  final value = _Private.value;
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types;

void run() {
  types.State.reset();
  final code = types.State.code;
  types.Extras.help();
  final extensionValue = types.Extras.value;
  final method = types.Factory.build;
  final constructor = types.Factory.named;
  final unnamed = types.Factory.new;
  types.Factory.named();
}
"#;

#[test]
fn resolves_all_indexed_static_member_owner_kinds() {
    let context = context();
    let reset = occurrence(CLIENT, "State.reset", "reset");
    let code = occurrence(CLIENT, "State.code", "code");
    let help = occurrence(CLIENT, "Extras.help", "help");
    let value = occurrence(CLIENT, "Extras.value", "value");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/client.dart", reset),
        DartDefinitionQuery::new("lib/client.dart", code),
        DartDefinitionQuery::new("lib/client.dart", help),
        DartDefinitionQuery::new("lib/client.dart", value),
    ]);

    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", reset),
        DartDeclarationKind::Method,
        "reset",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", code),
        DartDeclarationKind::Getter,
        "code",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", help),
        DartDeclarationKind::Method,
        "help",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", value),
        DartDeclarationKind::Getter,
        "value",
    );
}

#[test]
fn resolves_method_and_constructor_tear_offs_and_keyword_free_calls() {
    let context = context();
    let method = occurrence(CLIENT, "Factory.build;", "build");
    let constructor = occurrence(CLIENT, "Factory.named;", "named");
    let unnamed = occurrence(CLIENT, "Factory.new;", "new");
    let call = occurrence(CLIENT, "Factory.named();", "named");
    let instance = occurrence(TYPES, "this.work;", "work");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/client.dart", method),
        DartDefinitionQuery::new("lib/client.dart", constructor),
        DartDefinitionQuery::new("lib/client.dart", unnamed),
        DartDefinitionQuery::new("lib/client.dart", call),
        DartDefinitionQuery::new("lib/types.dart", instance),
    ]);

    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", method),
        DartDeclarationKind::Method,
        "build",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", constructor),
        DartDeclarationKind::Constructor,
        "Factory.named",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", unnamed),
        DartDeclarationKind::Constructor,
        "Factory",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/client.dart", call),
        DartDeclarationKind::Constructor,
        "Factory.named",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/types.dart", instance),
        DartDeclarationKind::Method,
        "work",
    );
}

#[test]
fn resolves_static_members_on_private_named_types_inside_the_library() {
    let context = context();
    let run = occurrence(TYPES, "_Private.run", "run");
    let value = occurrence(TYPES, "_Private.value", "value");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/types.dart", run),
        DartDefinitionQuery::new("lib/types.dart", value),
    ]);

    assert_target(
        resolution_at(&batch.resolutions, "lib/types.dart", run),
        DartDeclarationKind::Method,
        "run",
    );
    assert_target(
        resolution_at(&batch.resolutions, "lib/types.dart", value),
        DartDeclarationKind::Field,
        "value",
    );
}

fn context() -> DartWorkspaceResolutionContext {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    DartWorkspaceResolutionContext::new(&analysis)
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

fn assert_target(resolution: &DartDefinitionResolution, kind: DartDeclarationKind, name: &str) {
    assert_eq!(resolution.status, DartDefinitionResolutionStatus::Resolved);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, kind);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, "lib/types.dart");
        }
        target => panic!("unexpected target: {target:?}"),
    }
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
