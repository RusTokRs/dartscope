// Regression spec from the 2026-09-30 audit: lexical/metadata/literal behaviours claimed by the canonical-scanner refactor.
use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file;

fn names(source: &str) -> Vec<String> {
    analyze_file(DartFileInput::new("lib/a.dart", source)).declarations.iter().map(|d| format!("{:?}:{}", d.kind, d.name)).collect()
}

#[test]
#[ignore = "audit 2026-09-30 §4.1: a UTF-8 BOM hides the first declaration"]
fn bom_does_not_hide_the_first_declaration() {
    let with_bom = names("\u{feff}class First {}\nclass Second {}\n");
    println!("spec P1 BOM -> {with_bom:?}");
    assert!(with_bom.contains(&"Class:First".to_string()), "{with_bom:?}");
}

#[test]
#[ignore = "audit 2026-09-30 §4.2: enum constants and top-level accessors are not inventoried"]
fn enum_constants_and_top_level_accessors_are_inventoried() {
    let got = names("enum Color { red, green }\nint get total => 1;\nset total(int v) {}\n");
    println!("spec P2 enum/accessors -> {got:?}");
    assert!(got.iter().any(|n| n.ends_with(":red")), "enum constants missing: {got:?}");
    assert!(got.iter().any(|n| n.starts_with("Getter:")), "top-level getter missing: {got:?}");
}

#[test]
#[ignore = "audit 2026-09-30 §4.2: functions and methods with their own type parameters are not inventoried"]
fn generic_functions_and_methods_are_inventoried() {
    let got = names("T first<T>(List<T> items) => items.first;\nclass Box {\n  R map<R>(R a) => a;\n}\n");
    println!("spec P4 generic function/method -> {got:?}");
    assert!(got.iter().any(|n| n.ends_with(":first")), "generic top-level function missing: {got:?}");
    assert!(got.iter().any(|n| n.ends_with(":map")), "generic method missing: {got:?}");
}

#[test]
#[ignore = "audit 2026-09-30 §4.2: a function-type return type is parsed as a declaration named `Function`"]
fn function_type_return_does_not_create_a_bogus_declaration() {
    let got = names("void Function(int) make() => (i) {};\n");
    println!("spec P5 function-type return -> {got:?}");
    assert!(got.iter().any(|n| n.ends_with(":make")), "function missing: {got:?}");
    assert!(!got.iter().any(|n| n.ends_with(":Function")), "bogus `Function` declaration: {got:?}");
}

#[test]
fn nested_same_quote_interpolation_does_not_corrupt_following_declarations() {
    let got = names("const s = 'a ${b['c']} d';\nclass After {}\n");
    println!("spec P3 nested quotes -> {got:?}");
    assert!(got.contains(&"Class:After".to_string()), "{got:?}");
}
