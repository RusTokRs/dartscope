use dartscope_core::{Confidence, DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

#[test]
fn emits_only_binding_backed_simple_assignment_targets() {
    let source = r#"
void consume(Object? value) {}

void run(int value, int other, dynamic object, List<int> values, int index) {
  value = other;
  {
    var value = other;
    value = value;
  }
  value = other;
  value += other;
  value++;
  ++value;
  object.value = other;
  values[index] = other;
  (value, other) = pair;
  if (value == other) {
    consume(value);
  }
  [value].forEach((value) => value = other);
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/writes.dart", source));
    let writes: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::VariableWrite)
        .collect();

    let expected_offsets = [
        source.find("  value = other;").expect("parameter write") + 2,
        source
            .find("    value = value;")
            .expect("nested local write")
            + 4,
        source
            .rfind("  value = other;")
            .expect("parameter write after block")
            + 2,
    ];
    assert_eq!(writes.len(), expected_offsets.len());
    assert_eq!(
        writes
            .iter()
            .map(|reference| reference.span.byte_start)
            .collect::<Vec<_>>(),
        expected_offsets
    );
    for write in writes {
        assert_eq!(write.name, "value");
        assert_eq!(write.confidence, Confidence::High);
        assert_eq!(&source[write.span.byte_start..write.span.byte_end], "value");
        assert!(write.prefix.is_none());
        assert!(
            write
                .enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| symbol_id.ends_with("::function:run"))
        );
    }

    let encoded = serde_json::to_value(&analysis.references).expect("reference JSON");
    assert!(
        encoded
            .as_array()
            .expect("reference array")
            .iter()
            .any(
                |value| value.get("kind").and_then(serde_json::Value::as_str)
                    == Some("variable_write")
            )
    );
}

#[test]
fn keeps_assignment_rhs_reads_separate_from_the_write_target() {
    let source = r#"
void run(int value, int other) {
  value = value + other;
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/accesses.dart", source));
    let accesses: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::VariableRead
                    | DartIdentifierReferenceKind::VariableWrite
            )
        })
        .map(|reference| {
            (
                reference.kind,
                reference.name.as_str(),
                reference.span.byte_start,
            )
        })
        .collect();

    let target = source.find("value =").expect("assignment target");
    let rhs_value = source.rfind("value +").expect("value read");
    let rhs_other = source.rfind("other;").expect("other read");
    assert_eq!(
        accesses,
        [
            (DartIdentifierReferenceKind::VariableWrite, "value", target),
            (
                DartIdentifierReferenceKind::VariableRead,
                "value",
                rhs_value
            ),
            (
                DartIdentifierReferenceKind::VariableRead,
                "other",
                rhs_other
            ),
        ]
    );
}
