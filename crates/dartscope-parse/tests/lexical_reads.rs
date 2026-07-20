use dartscope_core::{Confidence, DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

#[test]
fn emits_only_binding_backed_unqualified_reads_with_exact_spans() {
    let source = r#"
void consume(Object? value, {Object? named}) {}

void run(int value, int other) {
  consume(value);
  final local = value;
  consume(local);
  {
    final value = other;
    consume(value);
  }
  consume(value);
  consume(named: value);
  value = other;
  value += other;
  value++;
  ++value;
  value.toString();
  final self = self;
  [value].forEach((value) => consume(value));
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/reads.dart", source));
    let reads: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableRead)
        .collect();

    let expected = [
        ("value", "consume(value)"),
        ("value", "final local = value"),
        ("local", "consume(local)"),
        ("other", "final value = other"),
        ("value", "consume(value)"),
        ("value", "consume(value)"),
        ("value", "named: value"),
        ("other", "value = other"),
        ("value", "value += other"),
        ("other", "value += other"),
        ("value", "value++"),
        ("value", "++value"),
        ("value", "value.toString"),
        ("value", "[value]"),
    ];
    assert_eq!(reads.len(), expected.len());
    for (read, (name, _evidence)) in reads.iter().zip(expected) {
        assert_eq!(read.name, name);
        assert_eq!(read.confidence, Confidence::High);
        assert_eq!(&source[read.span.byte_start..read.span.byte_end], read.name);
        assert!(read.prefix.is_none());
        assert!(
            read.enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| { symbol_id.ends_with("::function:run") })
        );
    }

    let simple_write = source.find("value = other").expect("plain assignment");
    assert!(
        reads
            .iter()
            .all(|read| read.span.byte_start != simple_write)
    );

    let combined_offsets = [
        source.find("value += other").expect("compound assignment"),
        source.find("value++").expect("postfix increment"),
        source.find("++value").expect("prefix increment") + 2,
    ];
    assert!(combined_offsets.iter().all(|offset| {
        reads
            .iter()
            .any(|read| read.name == "value" && read.span.byte_start == *offset)
    }));
    assert!(!reads.iter().any(|read| read.name == "self"));

    let closure_body = source
        .rfind("consume(value)")
        .expect("closure variable use")
        + "consume(".len();
    assert!(
        reads
            .iter()
            .all(|read| read.span.byte_start != closure_body)
    );

    let encoded = serde_json::to_value(&analysis.references).expect("reference JSON");
    assert!(
        encoded
            .as_array()
            .expect("reference array")
            .iter()
            .any(
                |value| value.get("kind").and_then(serde_json::Value::as_str)
                    == Some("variable_read")
            )
    );
}

#[test]
fn suppresses_unmodeled_loop_and_catch_shadowing() {
    let source = r#"
void consume(Object? value) {}

void run(int value) {
  for (final value in values) {
    consume(value);
  }
  try {
    consume(value);
  } catch (value, stack) {
    consume(value);
  }
  consume(value);
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/shadowing.dart", source));
    let reads: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableRead)
        .filter(|reference| reference.name == "value")
        .collect();

    let uses: Vec<_> = source
        .match_indices("consume(value)")
        .map(|(offset, _)| offset + "consume(".len())
        .collect();
    assert_eq!(uses.len(), 4);
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].span.byte_start, uses[1]);
    assert_eq!(reads[1].span.byte_start, uses[3]);
}

#[test]
fn suppresses_type_positions_that_share_a_visible_binding_name() {
    let source = r#"
void consume(Object? value) {}

void run(Object Object) {
  Object local;
  final copy = Object;
  if (copy is Object) {
    consume(copy);
  }
  final values = <Object>[Object];
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/types.dart", source));
    let reads: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableRead)
        .map(|reference| (reference.name.as_str(), reference.span.byte_start))
        .collect();

    let object_uses: Vec<_> = source.match_indices("Object").map(|(at, _)| at).collect();
    let copy_uses: Vec<_> = source.match_indices("copy").map(|(at, _)| at).collect();
    assert_eq!(object_uses.len(), 8);
    assert_eq!(copy_uses.len(), 3);
    assert_eq!(
        reads,
        [
            ("Object", object_uses[4]),
            ("copy", copy_uses[1]),
            ("copy", copy_uses[2]),
            ("Object", object_uses[7]),
        ]
    );
}
