// Regression spec from the 2026-09-30 audit: lexical/metadata/literal behaviours claimed by the canonical-scanner refactor.
use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file;

fn names(source: &str) -> Vec<String> {
    analyze_file(DartFileInput::new("lib/a.dart", source))
        .declarations
        .iter()
        .map(|d| format!("{:?}:{}", d.kind, d.name))
        .collect()
}

#[test]
fn bom_does_not_hide_the_first_declaration() {
    let with_bom = names("\u{feff}class First {}\nclass Second {}\n");
    println!("spec P1 BOM -> {with_bom:?}");
    assert!(
        with_bom.contains(&"Class:First".to_string()),
        "{with_bom:?}"
    );
}

#[test]
fn enum_constants_and_top_level_accessors_are_inventoried() {
    let got = names("enum Color { red, green }\nint get total => 1;\nset total(int v) {}\n");
    println!("spec P2 enum/accessors -> {got:?}");
    assert!(
        got.iter().any(|n| n.ends_with(":red")),
        "enum constants missing: {got:?}"
    );
    assert!(
        got.iter().any(|n| n.starts_with("Getter:")),
        "top-level getter missing: {got:?}"
    );
}

#[test]
fn generic_functions_and_methods_are_inventoried() {
    let got =
        names("T first<T>(List<T> items) => items.first;\nclass Box {\n  R map<R>(R a) => a;\n}\n");
    println!("spec P4 generic function/method -> {got:?}");
    assert!(
        got.iter().any(|n| n.ends_with(":first")),
        "generic top-level function missing: {got:?}"
    );
    assert!(
        got.iter().any(|n| n.ends_with(":map")),
        "generic method missing: {got:?}"
    );
}

#[test]
fn function_type_return_does_not_create_a_bogus_declaration() {
    let got = names("void Function(int) make() => (i) {};\n");
    println!("spec P5 function-type return -> {got:?}");
    assert!(
        got.iter().any(|n| n.ends_with(":make")),
        "function missing: {got:?}"
    );
    assert!(
        !got.iter().any(|n| n.ends_with(":Function")),
        "bogus `Function` declaration: {got:?}"
    );
}

#[test]
fn nested_same_quote_interpolation_does_not_corrupt_following_declarations() {
    let got = names("const s = 'a ${b['c']} d';\nclass After {}\n");
    println!("spec P3 nested quotes -> {got:?}");
    assert!(got.contains(&"Class:After".to_string()), "{got:?}");
}

#[test]
fn interpolation_with_inner_quotes_does_not_report_an_unterminated_string() {
    // From riverpod: the inner `"'"` used to end the outer literal and corrupt everything after it.
    let source = "String clean(String x) => '${x.replaceAll(\"'\", '')}';\nclass After {}\n";
    let analysis = analyze_file(DartFileInput::new("lib/a.dart", source));
    let codes: Vec<_> = analysis.diagnostics.iter().map(|d| d.code.as_str()).collect();
    println!("spec P3b interpolation with inner quotes -> {codes:?}");
    assert!(!codes.contains(&"unterminated_string"), "{codes:?}");
    assert!(
        analysis.declarations.iter().any(|d| d.name == "After"),
        "{:?}",
        analysis.declarations
    );
}
