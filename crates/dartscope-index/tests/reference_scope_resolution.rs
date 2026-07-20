use dartscope_core::{
    DartFileInput, DartIdentifierReferenceKind, DartProjectInput, DartSymbolResolutionStatus,
};
use dartscope_index::resolve_project_identifier_references;
use dartscope_parse::analyze_project_with_references;

#[test]
fn resolves_only_invocations_not_shadowed_by_a_visible_local() {
    let source = r#"
void target() {}

void run() {
  target();
  {
    final target = callback;
    target();
  }
  target();
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/main.dart", source)],
        vec![],
    ));

    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| reference.kind == DartIdentifierReferenceKind::InvocationTarget)
            .count(),
        2
    );
    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableRead)
            .count(),
        1
    );
    let resolved = resolve_project_identifier_references(&analysis);
    assert_eq!(resolved.resolutions.len(), 2);
    for resolution in &resolved.resolutions {
        assert_eq!(resolution.reference.name, "target");
        assert_eq!(resolution.status, DartSymbolResolutionStatus::Resolved);
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].declaration_path, "lib/main.dart");
        assert_eq!(resolution.candidates[0].name, "target");
    }

    let invocation_offsets: Vec<_> = resolved
        .resolutions
        .iter()
        .map(|resolution| resolution.reference.span.byte_start)
        .collect();
    let first = source.find("target();").expect("first invocation");
    let last = source.rfind("target();").expect("last invocation");
    assert_eq!(invocation_offsets, [first, last]);
}

#[test]
fn resolves_parser_produced_constructor_and_type_clause_facts() {
    let types = r#"
class Parent {
  const Parent.named();
}
mixin LocalMixin {}
abstract class Contract {}
"#;
    let client = r#"
import 'types.dart' as types;

class Child extends types.Parent with types.LocalMixin implements types.Contract {}

void make() {
  const types.Parent.named();
  types.Parent();
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", types),
            DartFileInput::new("lib/client.dart", client),
        ],
        vec![],
    ));
    let resolved = resolve_project_identifier_references(&analysis);

    let typed: Vec<_> = resolved
        .resolutions
        .iter()
        .filter(|resolution| {
            matches!(
                resolution.reference.kind,
                DartIdentifierReferenceKind::ConstructorTarget
                    | DartIdentifierReferenceKind::TypeAnnotation
            )
        })
        .collect();
    assert_eq!(typed.len(), 4);
    for resolution in typed {
        assert_eq!(resolution.status, DartSymbolResolutionStatus::Resolved);
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].declaration_path, "lib/types.dart");
    }

    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| {
                reference.kind == DartIdentifierReferenceKind::ConstructorTarget
            })
            .count(),
        1
    );
}

#[test]
fn resolves_parser_produced_declaration_type_positions() {
    let types = r#"
class Value {}
class Result {}
"#;
    let client = r#"
import 'types.dart' as types;

final types.Value top = seed;

types.Result build(types.Value input) {
  final types.Value local = input;
  return result;
}

class Holder {
  types.Value field = seed;

  types.Result convert(types.Value input) {
    return result;
  }
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", types),
            DartFileInput::new("lib/client.dart", client),
        ],
        vec![],
    ));
    let resolved = resolve_project_identifier_references(&analysis);
    let typed: Vec<_> = resolved
        .resolutions
        .iter()
        .filter(|resolution| {
            matches!(
                resolution.reference.kind,
                DartIdentifierReferenceKind::ParameterType
                    | DartIdentifierReferenceKind::ReturnType
                    | DartIdentifierReferenceKind::VariableType
            )
        })
        .collect();

    assert_eq!(typed.len(), 7);
    for resolution in typed {
        assert_eq!(resolution.status, DartSymbolResolutionStatus::Resolved);
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].declaration_path, "lib/types.dart");
        assert!(matches!(
            resolution.reference.name.as_str(),
            "Value" | "Result"
        ));
        assert_eq!(resolution.reference.prefix.as_deref(), Some("types"));
    }
}
