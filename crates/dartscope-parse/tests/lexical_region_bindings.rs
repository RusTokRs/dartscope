use dartscope_core::{DartFileInput, DartIdentifierReferenceKind, DartLexicalBindingKind};
use dartscope_parse::analyze_file_with_references;

const REGION_SOURCE: &str = r#"
void consume(Object? value) {}

void run(int value, Iterable<int> values) {
  final arrow = (int value) => value + 1;
  values.forEach((value) {
    value++;
    consume(value);
  });
  for (final value in values) {
    value += 1;
    consume(value);
  }
  for (var index = 0; index < 2; index++) {
    consume(index);
  }
  try {
    consume(value);
  } catch (value, stack) {
    value--;
    consume(stack);
  }
  consume(value);
}
"#;

#[test]
fn models_closure_loop_and_catch_bindings_with_exact_scopes() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/regions.dart", REGION_SOURCE));
    let region_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| {
            binding.symbol_id.contains("/closure_parameter:")
                || binding.symbol_id.contains("/for_variable:")
                || binding.symbol_id.contains("/catch_parameter:")
        })
        .collect();

    assert_eq!(region_bindings.len(), 6);
    assert_eq!(
        region_bindings
            .iter()
            .map(|binding| (binding.kind, binding.name.as_str()))
            .collect::<Vec<_>>(),
        [
            (DartLexicalBindingKind::Parameter, "value"),
            (DartLexicalBindingKind::Parameter, "value"),
            (DartLexicalBindingKind::LocalVariable, "value"),
            (DartLexicalBindingKind::LocalVariable, "index"),
            (DartLexicalBindingKind::LocalVariable, "value"),
            (DartLexicalBindingKind::LocalVariable, "stack"),
        ]
    );
    for binding in region_bindings {
        assert_eq!(
            &REGION_SOURCE[binding.declaration_span.byte_start..binding.declaration_span.byte_end],
            binding.name
        );
        assert!(binding.symbol_id.starts_with(&binding.enclosing_symbol_id));
        assert!(binding.enclosing_symbol_id.ends_with("::function:run"));
    }
}

#[test]
fn enables_binding_backed_accesses_inside_supported_regions() {
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/regions.dart", REGION_SOURCE));
    let read_only_offsets = [
        REGION_SOURCE.find("value + 1").expect("arrow body value"),
        REGION_SOURCE.find("index < 2").expect("classic condition"),
        REGION_SOURCE
            .find("consume(stack)")
            .expect("catch stack read")
            + "consume(".len(),
        REGION_SOURCE
            .rfind("consume(value)")
            .expect("outer value read")
            + "consume(".len(),
    ];
    let update_offsets = [
        REGION_SOURCE.find("value++").expect("closure update"),
        REGION_SOURCE.find("value += 1").expect("for-in update"),
        REGION_SOURCE.find("index++").expect("classic update"),
        REGION_SOURCE.find("value--").expect("catch update"),
    ];

    for offset in read_only_offsets {
        assert_eq!(
            kinds_at(&analysis.references, offset),
            vec![DartIdentifierReferenceKind::VariableRead]
        );
    }
    for offset in update_offsets {
        assert_eq!(
            kinds_at(&analysis.references, offset),
            vec![
                DartIdentifierReferenceKind::VariableRead,
                DartIdentifierReferenceKind::VariableWrite,
            ]
        );
    }

    let encoded = serde_json::to_value(&analysis.bindings).expect("binding JSON");
    let kinds: Vec<_> = encoded
        .as_array()
        .expect("binding array")
        .iter()
        .filter_map(|value| value.get("kind").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        kinds
            .iter()
            .all(|kind| matches!(*kind, "parameter" | "local_variable"))
    );
}

#[test]
fn models_parenthesized_closures_inside_concise_callables_without_switch_arm_bindings() {
    let source = r#"
int transform(int value) => [value].map((value) => value + 1).first;

int choose(int value) => switch (value) {
  0 => value,
  _ => value + 1,
};
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/concise.dart", source));
    let closure_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| binding.symbol_id.contains("/closure_parameter:"))
        .collect();

    assert_eq!(closure_bindings.len(), 1);
    assert_eq!(closure_bindings[0].name, "value");
    assert!(
        closure_bindings[0]
            .enclosing_symbol_id
            .ends_with("::function:transform")
    );
    let closure_read = source.find("value + 1").expect("closure read");
    assert_eq!(
        kinds_at(&analysis.references, closure_read),
        vec![DartIdentifierReferenceKind::VariableRead]
    );
}

#[test]
fn keeps_pattern_and_multi_declarator_for_regions_deferred() {
    let source = r#"
void run(
  int left,
  int right,
  Iterable<(int, int)> pairs,
  Iterable<int> values,
) {
  for (final (left, right) in pairs) {
    left++;
    right++;
  }
  for (var left = 0, right = 0; left < 1; left++) {
    right++;
  }
  for (var single = 0; single < 1; single++) single++;
  final collected = [for (var item in values) item];
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/deferred.dart", source));
    let deferred_offsets = [
        source.find("left++").expect("pattern left update"),
        source.find("right++").expect("pattern right update"),
        source.rfind("right++").expect("multi-declarator update"),
        source
            .rfind("single++")
            .expect("single-statement loop update"),
    ];

    assert!(
        analysis
            .bindings
            .iter()
            .all(|binding| !binding.symbol_id.contains("/for_variable:"))
    );
    assert!(
        deferred_offsets
            .iter()
            .all(|offset| kinds_at(&analysis.references, *offset).is_empty())
    );
}

fn kinds_at(
    references: &[dartscope_core::DartIdentifierReference],
    byte_start: usize,
) -> Vec<DartIdentifierReferenceKind> {
    references
        .iter()
        .filter(|reference| reference.span.byte_start == byte_start)
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::VariableRead
                    | DartIdentifierReferenceKind::VariableWrite
            )
        })
        .map(|reference| reference.kind)
        .collect()
}
