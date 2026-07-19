use dartscope_core::{Confidence, DartFileInput, DartIdentifierReferenceKind, DartProjectInput};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

#[test]
fn extracts_bounded_invocation_target_references_with_exact_spans() {
    let source = r#"
import 'api.dart' as api;

void run() {
  api.load();
  Factory.create().build();
  client.query();
  // Ignored.call();
  final text = 'Hidden.call()';
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/client.dart", source));

    assert_eq!(analysis.references.len(), 3);
    assert_eq!(analysis.references[0].name, "load");
    assert_eq!(analysis.references[0].prefix.as_deref(), Some("api"));
    assert_eq!(analysis.references[0].confidence, Confidence::High);
    assert_eq!(analysis.references[1].name, "Factory");
    assert_eq!(analysis.references[1].prefix, None);
    assert_eq!(analysis.references[1].confidence, Confidence::Medium);
    assert_eq!(analysis.references[2].name, "client");

    for reference in &analysis.references {
        assert_eq!(
            &source[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
        assert!(
            reference
                .enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| symbol_id.ends_with("::function:run"))
        );
    }
}

#[test]
fn suppresses_parameters_visible_locals_and_members_before_namespace_resolution() {
    let source = r#"
import 'api.dart' as api;

void local() {}
void external() {}

class Controller {
  void member() {}

  void run(dynamic scoped, dynamic api) {
    scoped();
    api.load();
    member();
    {
      final local = callback;
      local();
    }
    local();
    external();
  }
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/scopes.dart", source));

    assert_eq!(
        analysis
            .references
            .iter()
            .map(|reference| (reference.name.as_str(), reference.prefix.as_deref()))
            .collect::<Vec<_>>(),
        [("local", None), ("external", None)]
    );
    for reference in &analysis.references {
        assert_eq!(
            &source[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
        assert!(
            reference
                .enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| symbol_id.ends_with("/method:run"))
        );
    }
}

#[test]
fn local_declarations_do_not_retroactively_shadow_earlier_invocations() {
    let source = r#"
void target() {}

void run() {
  target();
  final target = callback;
  target();
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/order.dart", source));

    assert_eq!(analysis.references.len(), 1);
    assert_eq!(analysis.references[0].name, "target");
    assert_eq!(
        analysis.references[0].span.byte_start,
        source.find("target();").expect("first invocation")
    );
}

#[test]
fn project_reference_output_is_sorted_by_path_and_source_span() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/z.dart", "void z() { Zed(); }\n"),
            DartFileInput::new("lib/a.dart", "void a() { Alpha(); }\n"),
        ],
        vec![],
    ));

    assert_eq!(
        analysis
            .references
            .iter()
            .map(|reference| (reference.source_path.as_str(), reference.name.as_str()))
            .collect::<Vec<_>>(),
        [("lib/a.dart", "Alpha"), ("lib/z.dart", "Zed")]
    );
}

#[test]
fn extracts_explicit_constructor_and_nominal_type_clause_references() {
    let source = r#"
import 'types.dart' as types;

class LocalBase {}
mixin LocalMixin {}
class Child<T> extends types.Parent<T> with LocalMixin implements Contract<T> {}
mixin Guard<T> on types.Parent<T>, Contract<T> {}
extension Parsing<T> on types.Parent<T> {}
extension type UserId(int value) implements Contract<UserId> {}
class TypeParameter<T> extends T {}

void make() {
  new LocalBase();
  const types.Parent.named();
  Factory.create();
  Parent();
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/typed.dart", source));

    let type_references: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::TypeAnnotation)
        .map(|reference| (reference.name.as_str(), reference.prefix.as_deref()))
        .collect();
    assert_eq!(
        type_references,
        [
            ("Parent", Some("types")),
            ("LocalMixin", None),
            ("Contract", None),
            ("Parent", Some("types")),
            ("Contract", None),
            ("Parent", Some("types")),
            ("Contract", None),
        ]
    );
    assert!(!analysis.references.iter().any(|reference| {
        reference.kind == DartIdentifierReferenceKind::TypeAnnotation && reference.name == "T"
    }));

    let constructors: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::ConstructorTarget)
        .map(|reference| (reference.name.as_str(), reference.prefix.as_deref()))
        .collect();
    assert_eq!(
        constructors,
        [("LocalBase", None), ("Parent", Some("types"))]
    );
    assert!(!analysis.references.iter().any(|reference| {
        reference.kind == DartIdentifierReferenceKind::ConstructorTarget
            && matches!(reference.name.as_str(), "Factory" | "Parent")
            && reference.prefix.is_none()
    }));

    for reference in analysis.references.iter().filter(|reference| {
        matches!(
            reference.kind,
            DartIdentifierReferenceKind::ConstructorTarget
                | DartIdentifierReferenceKind::TypeAnnotation
        )
    }) {
        assert_eq!(
            &source[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
    }

    let encoded = serde_json::to_value(&analysis.references).expect("reference JSON");
    let kinds: Vec<_> = encoded
        .as_array()
        .expect("reference array")
        .iter()
        .filter_map(|value| value.get("kind").and_then(serde_json::Value::as_str))
        .collect();
    assert!(kinds.contains(&"constructor_target"));
    assert!(kinds.contains(&"type_annotation"));
}
