use dartscope_core::{DartDeclarationKind, DartFileAnalysis, DartFileInput};
use dartscope_parse::analyze_file;

fn analyze(source: &str) -> DartFileAnalysis {
    analyze_file(DartFileInput::new("lib/sample.dart", source))
}

fn kinds(analysis: &DartFileAnalysis) -> Vec<(&str, DartDeclarationKind)> {
    analysis
        .declarations
        .iter()
        .map(|declaration| (declaration.name.as_str(), declaration.kind))
        .collect()
}

fn diagnostic_codes(analysis: &DartFileAnalysis) -> Vec<&str> {
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[test]
fn named_factory_constructors_are_declarations_without_diagnostics() {
    let analysis = analyze(
        "class A {\n\
           factory A.fromJson(int value) => A();\n\
           factory A.create() { return A(); }\n\
           const factory A.aliased() = B;\n\
           const A();\n\
         }\n\
         class B { const B(); }\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![
            ("A", DartDeclarationKind::Class),
            ("A.fromJson", DartDeclarationKind::Constructor),
            ("A.create", DartDeclarationKind::Constructor),
            ("A.aliased", DartDeclarationKind::Constructor),
            ("A", DartDeclarationKind::Constructor),
            ("B", DartDeclarationKind::Class),
            ("B", DartDeclarationKind::Constructor),
        ]
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "factory constructors are ordinary Dart syntax: {:?}",
        diagnostic_codes(&analysis)
    );
}

#[test]
fn abstract_factory_declarations_are_collected() {
    let analysis = analyze("class C {\n  abstract factory C.create();\n}\n");

    assert_eq!(
        kinds(&analysis),
        vec![
            ("C", DartDeclarationKind::Class),
            ("C.create", DartDeclarationKind::Constructor),
        ]
    );
    assert!(analysis.diagnostics.is_empty());
}

#[test]
fn concise_constructor_syntax_is_diagnosed_and_not_fabricated() {
    let analysis = analyze("class D {\n  new(this.value);\n  int value = 0;\n}\n");

    assert_eq!(
        diagnostic_codes(&analysis),
        vec!["unsupported_concise_constructor"]
    );
    assert_eq!(
        kinds(&analysis),
        vec![
            ("D", DartDeclarationKind::Class),
            ("value", DartDeclarationKind::Field),
        ]
    );
}

#[test]
fn declarations_after_a_concise_constructor_on_the_same_line_are_kept() {
    let analysis = analyze("class D { new(int x) : this.x = x; int x; }\n");

    assert_eq!(
        diagnostic_codes(&analysis),
        vec!["unsupported_concise_constructor"]
    );
    assert_eq!(
        kinds(&analysis),
        vec![
            ("D", DartDeclarationKind::Class),
            ("x", DartDeclarationKind::Field),
        ]
    );
}

#[test]
fn ordinary_type_headers_never_report_unsupported_constructor_syntax() {
    let analysis = analyze(
        "class A<T extends Object> {}\n\
         class B extends A<List<int>> implements Comparable<B> {}\n\
         class C with M1, M2 {}\n\
         mixin M1 {}\n\
         mixin M2 {}\n\
         enum E { a }\n\
         extension J on List<int> {}\n\
         extension type K(int v) {}\n\
         sealed class L {}\n",
    );

    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostic_codes(&analysis)
    );
    assert_eq!(
        kinds(&analysis)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["A", "B", "C", "M1", "M2", "E", "J", "K", "L"]
    );
}
