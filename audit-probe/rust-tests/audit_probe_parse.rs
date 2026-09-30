// Audit probe: lexical/metadata/literal behaviours claimed by the canonical-scanner refactor.
use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file;

fn names(source: &str) -> Vec<String> {
    analyze_file(DartFileInput::new("lib/a.dart", source)).declarations.iter().map(|d| format!("{:?}:{}", d.kind, d.name)).collect()
}

#[test]
fn bom_does_not_hide_the_first_declaration() {
    let with_bom = names("\u{feff}class First {}\nclass Second {}\n");
    println!("AUDIT P1 BOM -> {with_bom:?}");
    assert!(with_bom.contains(&"Class:First".to_string()), "{with_bom:?}");
}

#[test]
fn enum_constants_and_top_level_accessors_are_inventoried() {
    let got = names("enum Color { red, green }\nint get total => 1;\nset total(int v) {}\n");
    println!("AUDIT P2 enum/accessors -> {got:?}");
    assert!(got.iter().any(|n| n.ends_with(":red")), "enum constants missing: {got:?}");
    assert!(got.iter().any(|n| n.starts_with("Getter:")), "top-level getter missing: {got:?}");
}

#[test]
fn nested_same_quote_interpolation_does_not_corrupt_following_declarations() {
    let got = names("const s = 'a ${b['c']} d';\nclass After {}\n");
    println!("AUDIT P3 nested quotes -> {got:?}");
    assert!(got.contains(&"Class:After".to_string()), "{got:?}");
}
