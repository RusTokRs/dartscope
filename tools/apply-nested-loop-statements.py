from pathlib import Path

controls = Path("crates/dartscope-parse/src/lexical_regions/controls.rs")
source = controls.read_text(encoding="utf-8")
old_call = "    let body_end = simple_statement_end(source, body_start)?;\n"
new_call = "    let body_end = statement_end(source, body_start)?;\n"
if source.count(old_call) != 1:
    raise SystemExit("expected one simple loop-body statement call")
source = source.replace(old_call, new_call, 1)
helper_start = source.index("fn simple_statement_end(")
helper_end = source.index("fn statement_end(", helper_start)
source = source[:helper_start] + source[helper_end:]
controls.write_text(source, encoding="utf-8")

parser_test = Path("crates/dartscope-parse/tests/lexical_region_bindings.rs")
source = parser_test.read_text(encoding="utf-8")
marker = "\nfn kinds_at(\n"
if source.count(marker) != 1:
    raise SystemExit("parser test helper marker not found")
test = r'''

#[test]
fn models_loop_bindings_inside_nested_unbraced_if_else_statements() {
    let source = r#"
void consume(int value) {}

void run(bool enabled, Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    if (enabled)
      consume(index);
    else
      index++;
  for (final value in values)
    if (enabled)
      consume(value);
    else
      value++;
  consume(index);
  consume(value);
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/nested_loops.dart", source));
    let loop_bindings: Vec<_> = analysis
        .bindings
        .iter()
        .filter(|binding| binding.symbol_id.contains("/for_variable:"))
        .collect();

    assert_eq!(
        loop_bindings
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        ["index", "value"]
    );
    assert!(loop_bindings.iter().all(|binding| {
        let scope = &source[binding.scope_span.byte_start..binding.scope_span.byte_end];
        scope.starts_with("if (enabled)") && scope.contains("else")
    }));

    let classic_read =
        source.find("consume(index)").expect("classic body read") + "consume(".len();
    let classic_update = source.rfind("index++").expect("classic else update");
    let for_in_read =
        source.find("consume(value)").expect("for-in body read") + "consume(".len();
    let for_in_update = source.find("value++").expect("for-in else update");
    for offset in [classic_read, classic_update, for_in_read, for_in_update] {
        assert!(
            kinds_at(&analysis.references, offset)
                .contains(&DartIdentifierReferenceKind::VariableRead)
        );
    }
    for offset in [classic_update, for_in_update] {
        assert!(
            kinds_at(&analysis.references, offset)
                .contains(&DartIdentifierReferenceKind::VariableWrite)
        );
    }
    for offset in [
        source.rfind("consume(index)").expect("post-loop index") + "consume(".len(),
        source.rfind("consume(value)").expect("post-loop value") + "consume(".len(),
    ] {
        assert!(kinds_at(&analysis.references, offset).is_empty());
    }
}
'''
source = source.replace(marker, test + marker, 1)
parser_test.write_text(source, encoding="utf-8")

index_test = Path("crates/dartscope-index/tests/lexical_region_resolution.rs")
source = index_test.read_text(encoding="utf-8")
marker = "\nfn assert_expected_bindings(\n"
if source.count(marker) != 1:
    raise SystemExit("index test helper marker not found")
test = r'''

#[test]
fn resolves_loop_accesses_inside_nested_unbraced_if_else_statements() {
    let source = r#"
void consume(int value) {}

void run(bool enabled, Iterable<int> values) {
  for (var index = 0; index < 2; index++)
    if (enabled)
      consume(index);
    else
      index++;
  for (final value in values)
    if (enabled)
      consume(value);
    else
      value++;
}
"#;
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/nested_loops.dart", source)],
        vec![],
    ));
    let classic_read =
        source.find("consume(index)").expect("classic body read") + "consume(".len();
    let classic_update = source.rfind("index++").expect("classic else update");
    let for_in_read =
        source.find("consume(value)").expect("for-in body read") + "consume(".len();
    let for_in_update = source.find("value++").expect("for-in else update");
    let read_offsets = [classic_read, classic_update, for_in_read, for_in_update];
    let write_offsets = [classic_update, for_in_update];

    let reads: Vec<_> = resolve_project_variable_read_references(&analysis)
        .into_iter()
        .filter(|resolution| read_offsets.contains(&resolution.query.byte_offset))
        .collect();
    let writes: Vec<_> = resolve_project_variable_write_references(&analysis)
        .into_iter()
        .filter(|resolution| write_offsets.contains(&resolution.query.byte_offset))
        .collect();

    assert_eq!(reads.len(), read_offsets.len());
    assert_eq!(writes.len(), write_offsets.len());
    for resolution in reads.iter().chain(&writes) {
        assert_eq!(
            resolution.status,
            DartLexicalBindingResolutionStatus::Resolved
        );
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(
            resolution.candidates[0].kind,
            DartLexicalBindingKind::LocalVariable
        );
        assert!(
            resolution.candidates[0]
                .symbol_id
                .contains("/for_variable:")
        );
    }
}
'''
source = source.replace(marker, test + marker, 1)
index_test.write_text(source, encoding="utf-8")
