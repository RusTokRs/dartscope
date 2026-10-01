//! Declaration forms the conservative inventory must recognize: generic and function-typed callables,
//! enum constants, top-level accessors, and the `extends`/`with`/`on` clauses of type headers.

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartFileInput};
use dartscope_parse::analyze_file;

fn analyze(source: &str) -> DartFileAnalysis {
    analyze_file(DartFileInput::new("lib/sample.dart", source))
}

fn labels(analysis: &DartFileAnalysis) -> Vec<String> {
    analysis
        .declarations
        .iter()
        .map(|declaration| format!("{:?}:{}", declaration.kind, declaration.name))
        .collect()
}

fn find<'a>(
    analysis: &'a DartFileAnalysis,
    kind: DartDeclarationKind,
    name: &str,
) -> &'a DartDeclaration {
    analysis
        .declarations
        .iter()
        .find(|declaration| declaration.kind == kind && declaration.name == name)
        .unwrap_or_else(|| panic!("missing {kind:?} {name}: {:?}", labels(analysis)))
}

#[test]
fn generic_functions_methods_and_getters_are_inventoried() {
    let analysis = analyze(
        "T first<T>(List<T> items) => items.first;\n\
         Map<String, List<int>> table<U>(U a) => {};\n\
         Future<T?> load<T extends Object>() async { return null; }\n\
         class Box {\n  R map<R>(R a) => a;\n  Iterable<R> expand<R>(R Function(int) f) sync* {}\n  Map<String, int> get counts => {'a': 1};\n}\n",
    );

    assert_eq!(
        labels(&analysis),
        [
            "Function:first",
            "Function:table",
            "Function:load",
            "Class:Box",
            "Method:map",
            "Method:expand",
            "Getter:counts",
        ]
    );
}

#[test]
fn an_arrow_body_that_starts_with_a_brace_ends_at_the_semicolon() {
    let source = "Map<String, int> get counts => {'a': 1};\nint other() => 2;\n";
    let analysis = analyze(source);

    let counts = find(&analysis, DartDeclarationKind::Getter, "counts");
    let span = counts.declaration_span.as_ref().expect("declaration span");
    assert_eq!(&source[span.byte_start..span.byte_end], &source[..source.find('\n').unwrap()]);
    find(&analysis, DartDeclarationKind::Function, "other");
}

#[test]
fn function_and_record_return_types_do_not_hide_the_declared_name() {
    let analysis = analyze(
        "void Function(int) make() => (i) {};\n\
         Function(int)? maybe() => null;\n\
         List<void Function(int)> handlers() => [];\n\
         (int, int) pair() => (1, 2);\n\
         ({int a, int b}) named() => (a: 1, b: 2);\n\
         class Factory {\n  void Function(int) build() => (i) {};\n  (int, int) pair() => (1, 2);\n}\n",
    );

    assert_eq!(
        labels(&analysis),
        [
            "Function:make",
            "Function:maybe",
            "Function:handlers",
            "Function:pair",
            "Function:named",
            "Class:Factory",
            "Method:build",
            "Method:pair",
        ]
    );
}

#[test]
fn function_typed_variables_and_fields_are_inventoried_by_their_own_name() {
    let analysis = analyze(
        "void Function(int) callback = (i) {};\n\
         final Widget Function(BuildContext)? builder = null;\n\
         (int, int) origin = (0, 0);\n\
         class Panel {\n  final void Function(String)? onChanged;\n  final Widget Function(BuildContext) builder;\n  final (int, int) point;\n  Panel(this.onChanged, this.builder, this.point);\n}\n",
    );

    assert_eq!(
        labels(&analysis),
        [
            "Variable:callback",
            "Variable:builder",
            "Variable:origin",
            "Class:Panel",
            "Field:onChanged",
            "Field:builder",
            "Field:point",
            "Constructor:Panel",
        ]
    );
}

#[test]
fn statements_with_parentheses_are_not_mistaken_for_typed_declarations() {
    let analysis = analyze(
        "void run(Function f) {\n  f(1);\n  (a, b) = (b, a);\n  else_branch(2) y;\n  Function.apply(f, []);\n  var (x, z) = pair();\n}\n",
    );

    assert_eq!(labels(&analysis), ["Function:run"]);
}

#[test]
fn enum_constants_are_inventoried_as_fields_of_the_enum() {
    let analysis = analyze(
        "enum Color { red, green, blue }\n\
         enum Planet {\n  mercury(1),\n  @deprecated\n  venus(2),\n  earth(3);\n\n  final int order;\n  const Planet(this.order);\n  bool get inner => order < 3;\n}\n\
         enum Flag { on, off, }\n\
         enum Box<T> { a<int>(1), b<String>('x'); final T value; const Box(this.value); }\n",
    );

    assert_eq!(
        labels(&analysis),
        [
            "Enum:Color",
            "Field:red",
            "Field:green",
            "Field:blue",
            "Enum:Planet",
            "Field:mercury",
            "Field:venus",
            "Field:earth",
            "Field:order",
            "Constructor:Planet",
            "Getter:inner",
            "Enum:Flag",
            "Field:on",
            "Field:off",
            "Enum:Box",
            "Field:a",
            "Field:b",
            "Field:value",
            "Constructor:Box",
        ]
    );

    let planet = find(&analysis, DartDeclarationKind::Enum, "Planet");
    let venus = find(&analysis, DartDeclarationKind::Field, "venus");
    assert_eq!(venus.parent_symbol_id, planet.symbol_id);
    assert_eq!(
        venus.symbol_id.as_deref(),
        Some("lib/sample.dart::enum:Planet/field:venus")
    );
    assert_eq!(venus.span.start_line, 5, "the line of the constant, not of its annotation");
    let full = venus.declaration_span.as_ref().expect("declaration span");
    assert_eq!((full.start_line, full.end_line), (5, 5));
}

#[test]
fn top_level_getters_and_setters_are_inventoried() {
    let analysis = analyze(
        "int get total => 1;\n\
         set total(int value) {}\n\
         List<int> get items => const [];\n\
         external String get name;\n\
         final get = 1;\n",
    );

    assert_eq!(
        labels(&analysis),
        [
            "Getter:total",
            "Setter:total",
            "Getter:items",
            "Getter:name",
            "Variable:get",
        ]
    );
    assert_eq!(
        find(&analysis, DartDeclarationKind::Setter, "total")
            .symbol_id
            .as_deref(),
        Some("lib/sample.dart::setter:total")
    );
}

#[test]
fn on_clauses_are_not_reported_as_base_classes_or_mixed_in_types() {
    let analysis = analyze(
        "class A {}\n\
         mixin M on A, B<int> {}\n\
         class C extends A with M, N implements I {}\n\
         extension X on String {}\n\
         extension<T> on T {}\n\
         extension Y<T extends Object> on T {}\n\
         extension Z<K, V> on Map<K, V> {}\n\
         extension on\n    List<int> {}\n\
         enum E with M implements I { a }\n\
         mixin G<K, V> on Map<K, V> {}\n",
    );

    let mixin = find(&analysis, DartDeclarationKind::Mixin, "M");
    assert_eq!(mixin.extends, None);
    assert!(mixin.mixes_in.is_empty());
    assert_eq!(mixin.on_types, ["A", "B"]);

    let class = find(&analysis, DartDeclarationKind::Class, "C");
    assert_eq!(class.extends.as_deref(), Some("A"));
    assert_eq!(class.mixes_in, ["M", "N"]);
    assert!(class.on_types.is_empty());

    let extension = find(&analysis, DartDeclarationKind::Extension, "X");
    assert_eq!(extension.extends, None);
    assert_eq!(extension.on_types, ["String"]);

    let own_parameter = find(&analysis, DartDeclarationKind::Extension, "Y");
    assert!(
        own_parameter.on_types.is_empty(),
        "an `on` type that is the extension's own type parameter applies to every receiver"
    );
    assert_eq!(
        find(&analysis, DartDeclarationKind::Extension, "Z").on_types,
        ["Map"]
    );

    let unnamed: Vec<_> = analysis
        .declarations
        .iter()
        .filter(|d| d.kind == DartDeclarationKind::Extension && d.name.is_empty())
        .map(|d| d.on_types.clone())
        .collect();
    assert_eq!(unnamed, [vec![], vec!["List".to_string()]]);

    assert_eq!(
        find(&analysis, DartDeclarationKind::Enum, "E").mixes_in,
        ["M"]
    );
    assert_eq!(
        find(&analysis, DartDeclarationKind::Mixin, "G").on_types,
        ["Map"]
    );
}

#[test]
fn declarations_without_an_on_clause_serialize_without_the_on_types_field() {
    let analysis = analyze("class A {}\nmixin M on A {}\n");
    let value = serde_json::to_value(&analysis.declarations).expect("serialize");

    assert!(value[0].get("on_types").is_none(), "{value}");
    assert_eq!(value[1]["on_types"], serde_json::json!(["A"]));
}

#[test]
fn a_byte_order_mark_does_not_hide_the_first_line() {
    let source = "\u{feff}import 'a.dart';\nclass First {}\n";
    let analysis = analyze(source);

    assert_eq!(analysis.imports.len(), 1, "{:?}", analysis.imports);
    let first = find(&analysis, DartDeclarationKind::Class, "First");
    assert_eq!(first.span.start_line, 2);

    let source = "\u{feff}class First {}\n";
    let analysis = analyze(source);
    let first = find(&analysis, DartDeclarationKind::Class, "First");
    let span = first.declaration_span.as_ref().expect("declaration span");
    assert_eq!((span.start_line, span.start_column), (1, 1));
    assert_eq!(span.byte_start, "\u{feff}".len());
}
