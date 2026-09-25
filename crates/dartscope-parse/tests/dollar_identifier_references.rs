//! Reference-side coverage for `$` identifiers.
//!
//! Declaration names keep their dollar signs (see `declaration_inventory_dollar_names.rs`), so every
//! reference scanner must scan the same character set. Otherwise a spelling such as `render$` is
//! truncated to `render`, which silently matches an unrelated declaration or produces a spurious
//! missing-definition result.

use dartscope_core::{DartFileInput, DartIdentifierReference, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
class Widget$Base {
  int count$ = 0;

  void render$() {}

  void exercise() {
    render$();
    count$ = count$ + 1;
  }
}

void _$buildFromJson() {}

void use() {
  _$buildFromJson();
}

const query$ = r'''
query GetUser {
  viewer {
    id
  }
}
''';
"#;

fn reference_named<'analysis>(
    references: &'analysis [DartIdentifierReference],
    name: &str,
    kind: DartIdentifierReferenceKind,
) -> &'analysis DartIdentifierReference {
    references
        .iter()
        .find(|reference| reference.name == name && reference.kind == kind)
        .unwrap_or_else(|| {
            panic!(
                "missing {kind:?} reference {name:?}; found {:?}",
                references
                    .iter()
                    .map(|reference| (reference.name.as_str(), reference.kind))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn exact_owner_member_facts_keep_dollar_names() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));
    let owner = "lib/sample.dart::class:Widget$Base";

    let invocation = reference_named(
        &analysis.references,
        "render$",
        DartIdentifierReferenceKind::MemberInvocationInstance,
    );
    assert_eq!(invocation.prefix.as_deref(), Some(owner));

    let write = reference_named(
        &analysis.references,
        "count$",
        DartIdentifierReferenceKind::MemberPropertyWriteInstance,
    );
    assert_eq!(write.prefix.as_deref(), Some(owner));
    reference_named(
        &analysis.references,
        "count$",
        DartIdentifierReferenceKind::MemberPropertyReadInstance,
    );

    assert!(
        analysis
            .references
            .iter()
            .all(|reference| reference.name != "render" && reference.name != "count"),
        "a dollar name must never be reported truncated"
    );
}

#[test]
fn top_level_call_to_a_private_dollar_function_keeps_its_full_name() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    let call = reference_named(
        &analysis.references,
        "_$buildFromJson",
        DartIdentifierReferenceKind::InvocationTarget,
    );
    assert_eq!(
        call.enclosing_symbol_id.as_deref(),
        Some("lib/sample.dart::function:use")
    );
    assert_eq!(
        call.span.byte_start,
        SOURCE.find("_$buildFromJson();").expect("call site")
    );
}

#[test]
fn dart_constant_name_keeps_its_dollar_sign_next_to_a_graphql_operation_name() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/sample.dart", SOURCE));

    let operation = analysis
        .file
        .graphql_operations
        .iter()
        .find(|operation| operation.operation_name.as_deref() == Some("GetUser"))
        .expect("graphql operation");
    assert_eq!(operation.constant_name, "query$");
}
