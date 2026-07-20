use dartscope_core::{DartFileInput, DartLexicalBindingKind};
use dartscope_parse::analyze_file_with_references;

#[test]
fn emits_parameter_and_local_bindings_with_exact_scopes() {
    let source = r#"
void run(
  int parameter,
  {required String named, final untyped, int _}
) {
  final first = parameter;
  use(first);
  {
    var shadow = first, second = shadow;
    use(shadow);
    use(second);
  }
  use(first);
}

class Box {
  final int field;

  Box(this.field, int explicit) : field = explicit {
    final local = explicit;
    use(local);
  }
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/bindings.dart", source));

    let names: Vec<_> = analysis
        .bindings
        .iter()
        .map(|binding| (binding.kind, binding.name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            (DartLexicalBindingKind::Parameter, "parameter"),
            (DartLexicalBindingKind::Parameter, "named"),
            (DartLexicalBindingKind::Parameter, "untyped"),
            (DartLexicalBindingKind::LocalVariable, "first"),
            (DartLexicalBindingKind::LocalVariable, "shadow"),
            (DartLexicalBindingKind::LocalVariable, "second"),
            (DartLexicalBindingKind::Parameter, "explicit"),
            (DartLexicalBindingKind::LocalVariable, "local"),
        ]
    );
    assert!(!analysis.bindings.iter().any(|binding| binding.name == "_"));
    assert!(
        !analysis
            .bindings
            .iter()
            .any(|binding| binding.name == "field")
    );

    for binding in &analysis.bindings {
        assert_eq!(
            &source[binding.declaration_span.byte_start..binding.declaration_span.byte_end],
            binding.name
        );
        assert!(binding.scope_span.byte_start <= binding.scope_span.byte_end);
        assert!(binding.symbol_id.starts_with(&binding.enclosing_symbol_id));
    }

    let first = analysis
        .bindings
        .iter()
        .find(|binding| binding.name == "first")
        .expect("first binding");
    let shadow = analysis
        .bindings
        .iter()
        .find(|binding| binding.name == "shadow")
        .expect("shadow binding");
    let inner_use = source.find("use(shadow)").expect("inner use") + 4;
    let outer_use = source.rfind("use(first)").expect("outer use") + 4;
    assert!(shadow.scope_span.byte_start <= inner_use && inner_use < shadow.scope_span.byte_end);
    assert!(first.scope_span.byte_start <= outer_use && outer_use < first.scope_span.byte_end);
    assert!(outer_use >= shadow.scope_span.byte_end);

    let encoded = serde_json::to_value(&analysis.bindings).expect("binding JSON");
    let kinds: Vec<_> = encoded
        .as_array()
        .expect("binding array")
        .iter()
        .filter_map(|value| value.get("kind").and_then(serde_json::Value::as_str))
        .collect();
    assert!(kinds.contains(&"parameter"));
    assert!(kinds.contains(&"local_variable"));
}

#[test]
fn pure_analysis_remains_free_of_binding_facts() {
    let source = "void run(int value) { final local = value; }";
    let analysis = dartscope_parse::analyze_file(DartFileInput::new("lib/pure.dart", source));
    let encoded = serde_json::to_value(analysis).expect("pure JSON");
    assert!(encoded.get("bindings").is_none());
}
