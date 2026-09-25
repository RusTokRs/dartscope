use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartFileInput};
use dartscope_parse::analyze_file;

fn declarations(source: &str) -> DartFileAnalysis {
    analyze_file(DartFileInput::new("lib/sample.dart", source))
}

fn named<'analysis>(
    analysis: &'analysis DartFileAnalysis,
    name: &str,
    kind: DartDeclarationKind,
) -> &'analysis DartDeclaration {
    analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == name && declaration.kind == kind)
        .unwrap_or_else(|| {
            panic!(
                "missing {kind:?} declaration for {name}; found {:?}",
                analysis
                    .declarations
                    .iter()
                    .map(|declaration| (declaration.name.as_str(), declaration.kind))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn type_body_on_the_header_line_still_collects_members() {
    let analysis = declarations("class A { final int count = 0; int run() { return count; } }\n");

    assert_eq!(analysis.declarations.len(), 3);
    let owner = named(&analysis, "A", DartDeclarationKind::Class);
    for (name, kind) in [
        ("count", DartDeclarationKind::Field),
        ("run", DartDeclarationKind::Method),
    ] {
        let declaration = named(&analysis, name, kind);
        assert_eq!(
            declaration.parent_symbol_id.as_deref(),
            owner.symbol_id.as_deref()
        );
        assert_eq!(declaration.span.start_line, 1);
    }
    let method = named(&analysis, "run", DartDeclarationKind::Method);
    assert_eq!(
        method.declaration_span.as_ref().map(|span| span.end_line),
        Some(1)
    );
    assert_eq!(
        method.declaration_span.as_ref().map(|span| span.end_column),
        Some(59)
    );
}

#[test]
fn member_body_opening_on_the_header_line_is_collected() {
    let analysis = declarations("class A { int run() {\n    return 1;\n  }\n}\n");

    let method = named(&analysis, "run", DartDeclarationKind::Method);
    assert_eq!(method.span.start_line, 1);
    assert_eq!(
        method.declaration_span.as_ref().map(|span| span.end_line),
        Some(3)
    );
}

#[test]
fn second_member_on_one_line_is_collected() {
    let analysis =
        declarations("class A {\n  final int count = 0; int run() { return count; }\n}\n");

    assert_eq!(analysis.declarations.len(), 3);
    assert_eq!(
        named(&analysis, "run", DartDeclarationKind::Method)
            .declaration_span
            .as_ref()
            .map(|span| span.end_column),
        Some(51)
    );
}

#[test]
fn top_level_declarations_share_a_line() {
    let analysis = declarations("import 'dart:io'; class A {} void main() {}\n");

    assert_eq!(analysis.declarations.len(), 2);
    // The compatibility `span` stays anchored to the declaration's source line even when the
    // declaration does not start that line, while `declaration_span` keeps the exact position.
    let class = named(&analysis, "A", DartDeclarationKind::Class);
    assert_eq!(class.span.start_line, 1);
    assert_eq!(
        class
            .declaration_span
            .as_ref()
            .map(|span| span.start_column),
        Some(19)
    );
    let function = named(&analysis, "main", DartDeclarationKind::Function);
    assert_eq!(function.span.start_line, 1);
    assert_eq!(
        function
            .declaration_span
            .as_ref()
            .map(|span| span.start_column),
        Some(30)
    );
}

#[test]
fn local_variable_on_the_callable_header_line_is_collected() {
    let analysis = declarations("void f() { var a = 1; }\n");

    let owner = named(&analysis, "f", DartDeclarationKind::Function);
    let local = named(&analysis, "a", DartDeclarationKind::LocalVariable);
    assert_eq!(
        local.parent_symbol_id.as_deref(),
        owner.symbol_id.as_deref()
    );
    assert_eq!(local.span.start_line, 1);
}

#[test]
fn explicitly_typed_and_late_top_level_variables_are_collected() {
    let analysis = declarations(
        "int counter = 0;\n\
         late String title = 'x';\n\
         late final int total = 1;\n\
         int first, second;\n",
    );

    let names: Vec<_> = analysis
        .declarations
        .iter()
        .map(|declaration| (declaration.name.as_str(), declaration.kind))
        .collect();
    assert_eq!(
        names,
        vec![
            ("counter", DartDeclarationKind::Variable),
            ("title", DartDeclarationKind::Variable),
            ("total", DartDeclarationKind::Variable),
            ("first", DartDeclarationKind::Variable),
            ("second", DartDeclarationKind::Variable),
        ]
    );
    assert_eq!(
        named(&analysis, "second", DartDeclarationKind::Variable)
            .span
            .start_line,
        4
    );
}

#[test]
fn getter_and_setter_headers_are_not_reported_as_functions() {
    let analysis = declarations("String get label => 'x';\nset label(String value) {}\n");

    assert!(
        analysis.declarations.is_empty(),
        "getter and setter headers must not fabricate top-level functions: {:?}",
        analysis
            .declarations
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn constructor_calls_inside_a_one_line_body_are_not_declarations() {
    let analysis = declarations("class A { A(); }\n");

    assert_eq!(analysis.declarations.len(), 2);
    assert_eq!(
        named(&analysis, "A", DartDeclarationKind::Constructor)
            .span
            .start_line,
        1
    );
}
