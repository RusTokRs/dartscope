use dartscope_core::{Confidence, DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

#[test]
fn normative_compound_assignments_and_increments_emit_paired_access_facts() {
    let operators = [
        "+=", "-=", "*=", "/=", "%=", "~/=", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "??=",
    ];
    let mut source = String::from("void run(int value, int other) {\n");
    let mut target_offsets = Vec::new();
    for operator in operators {
        let line_start = source.len();
        source.push_str(&format!("  value {operator} other;\n"));
        target_offsets.push(line_start + 2);
    }
    for statement in ["value++;", "value--;", "++value;", "--value;"] {
        let line_start = source.len();
        source.push_str("  ");
        source.push_str(statement);
        source.push('\n');
        target_offsets.push(line_start + if statement.starts_with("value") { 2 } else { 4 });
    }
    source.push_str("}\n");

    let analysis = analyze_file_with_references(DartFileInput::new("lib/updates.dart", &source));
    let value_accesses: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.name == "value")
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::VariableRead
                    | DartIdentifierReferenceKind::VariableWrite
            )
        })
        .collect();

    assert_eq!(value_accesses.len(), target_offsets.len() * 2);
    for (accesses, expected_offset) in value_accesses.chunks_exact(2).zip(target_offsets) {
        assert_eq!(
            accesses
                .iter()
                .map(|reference| reference.kind)
                .collect::<Vec<_>>(),
            [
                DartIdentifierReferenceKind::VariableRead,
                DartIdentifierReferenceKind::VariableWrite,
            ]
        );
        for reference in accesses {
            assert_eq!(reference.span.byte_start, expected_offset);
            assert_eq!(
                &source[reference.span.byte_start..reference.span.byte_end],
                "value"
            );
            assert_eq!(reference.confidence, Confidence::High);
            assert!(reference.prefix.is_none());
            assert!(
                reference
                    .enclosing_symbol_id
                    .as_deref()
                    .is_some_and(|symbol_id| symbol_id.ends_with("::function:run"))
            );
        }
    }

    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| {
                reference.kind == DartIdentifierReferenceKind::VariableRead
                    && reference.name == "other"
            })
            .count(),
        operators.len()
    );
}

#[test]
fn heuristic_combined_updates_support_loop_bindings_but_omit_members_indices_and_patterns() {
    let source = r#"
void run(
  int value,
  int other,
  dynamic object,
  List<int> values,
  int index,
  Iterable<(int, int)> pairs,
) {
  object.value += other;
  ++object.value;
  values[index]++;
  --values[index];
  (value, other) = pair;
  for (final (value, other) in pairs) {
    value += other;
  }
  for (var value = 0, other = 0; value < 1; value++) {
    other++;
  }
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/deferred.dart", source));

    let unsupported_write_offsets = [
        occurrence(source, "object.value += other", "value"),
        occurrence(source, "++object.value", "value"),
        occurrence(source, "values[index]++", "values"),
        occurrence(source, "--values[index]", "values"),
        occurrence(source, "(value, other) = pair", "value"),
        occurrence(source, "(value, other) = pair", "other"),
        occurrence(source, "value += other", "value"),
    ];
    for offset in unsupported_write_offsets {
        assert!(!analysis.references.iter().any(|reference| {
            reference.kind == DartIdentifierReferenceKind::VariableWrite
                && reference.span.byte_start == offset
        }));
    }

    for offset in [
        source.rfind("value++").expect("classic loop update"),
        source.rfind("other++").expect("classic loop body update"),
    ] {
        assert_eq!(
            kinds_at(&analysis.references, offset),
            vec![
                DartIdentifierReferenceKind::VariableRead,
                DartIdentifierReferenceKind::VariableWrite,
            ]
        );
    }
}

#[test]
fn plain_assignment_targets_remain_write_only() {
    let source = r#"
void run(int value, int other) {
  value = other;
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/plain.dart", source));
    let target = source.find("value =").expect("plain assignment target");
    let target_accesses: Vec<_> = analysis
        .references
        .iter()
        .filter(|reference| reference.span.byte_start == target)
        .map(|reference| reference.kind)
        .collect();

    assert_eq!(
        target_accesses,
        [DartIdentifierReferenceKind::VariableWrite]
    );
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
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
