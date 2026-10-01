// Regression spec from the 2026-09-30 audit: does member navigation fabricate or miss targets? Expected-correct assertions; a failure is a finding.
use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolutionStatus, DartDefinitionTarget,
    DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

fn resolve(
    files: &[(&str, &str)],
    file: &str,
    fragment: &str,
    token: &str,
) -> (DartDefinitionResolutionStatus, Vec<String>) {
    let source = files
        .iter()
        .find(|(path, _)| *path == file)
        .expect("file")
        .1;
    let start = source.find(fragment).expect("fragment");
    let offset = start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token");
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        files
            .iter()
            .map(|(p, s)| DartFileInput::new(*p, *s))
            .collect(),
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let batch = context.find_definitions(&[DartDefinitionQuery::new(file, offset)]);
    let resolution = &batch.resolutions[0];
    let targets = resolution
        .targets
        .iter()
        .map(|target| match target {
            DartDefinitionTarget::Namespace(c) => format!(
                "{}:{:?}:{} basis={:?}",
                c.declaration_path, c.kind, c.name, c.basis
            ),
            DartDefinitionTarget::Lexical(b) => format!("lexical {}", b.name),
        })
        .collect();
    (resolution.status, targets)
}

#[test]
#[ignore = "audit 2026-09-30 §5.1: extension on an unrelated type is used as a definition"]
fn unrelated_extension_member_is_not_a_definition_for_this_member() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void run() { this.zap(); }\n}\nextension StringX on String {\n  int zap() => 1;\n}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.zap", "zap");
    println!(
        "spec S1 this.zap() where only `extension on String` declares zap -> {status:?} {targets:?}"
    );
    assert_ne!(
        status,
        DartDefinitionResolutionStatus::Resolved,
        "extension on an unrelated type was used as the definition: {targets:?}"
    );
}

#[test]
#[ignore = "audit 2026-09-30 §5.1: unimported extension resolved with basis DirectImport"]
fn extension_from_an_unimported_library_is_not_a_definition() {
    let files = [
        (
            "lib/ext.dart",
            "class Foo {}\nextension FooX on Foo {\n  void extra() {}\n}\n",
        ),
        (
            "lib/a.dart",
            "class Bar {\n  void go() { this.extra(); }\n}\n",
        ),
    ];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.extra", "extra");
    println!(
        "spec S3 this.extra() with extension in a library a.dart never imports -> {status:?} {targets:?}"
    );
    assert_ne!(
        status,
        DartDefinitionResolutionStatus::Resolved,
        "unimported, unrelated extension resolved: {targets:?}"
    );
}

#[test]
#[ignore = "audit 2026-09-30 §5.3: only one inheritance level is followed"]
fn inherited_member_from_a_grandparent_is_resolved() {
    let files = [(
        "lib/a.dart",
        "class A {\n  void hello() {}\n}\nclass B extends A {}\nclass C extends B {\n  void go() { this.hello(); }\n}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.hello", "hello");
    println!("spec S2 this.hello() declared two levels up -> {status:?} {targets:?}");
    assert_eq!(
        status,
        DartDefinitionResolutionStatus::Resolved,
        "multi-level inheritance not followed: {targets:?}"
    );
}

#[test]
fn inherited_member_from_a_direct_parent_is_resolved() {
    let files = [(
        "lib/a.dart",
        "class A {\n  void hello() {}\n}\nclass B extends A {\n  void go() { this.hello(); }\n}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.hello", "hello");
    println!("spec S4 this.hello() declared in direct parent -> {status:?} {targets:?}");
    assert_eq!(status, DartDefinitionResolutionStatus::Resolved);
}

#[test]
fn class_member_wins_over_same_named_extension_member() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void run() {}\n  void go() { this.run(); }\n}\nextension FooX on Foo {\n  void run() {}\n}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.run", "run");
    println!("spec S5 class member vs extension member -> {status:?} {targets:?}");
    assert_eq!(status, DartDefinitionResolutionStatus::Resolved);
    assert_eq!(targets.len(), 1, "{targets:?}");
}

#[test]
fn mixin_on_constraint_is_not_reported_as_mixed_in_type() {
    let file = dartscope_parse::analyze_file(DartFileInput::new(
        "lib/m.dart",
        "class A {}\nmixin M on A {}\nclass C extends A with M {}\nextension X on String {}\n",
    ));
    let summary: Vec<String> = file
        .declarations
        .iter()
        .filter(|d| d.parent_symbol_id.is_none())
        .map(|d| {
            format!(
                "{:?}:{} extends={:?} mixes_in={:?}",
                d.kind, d.name, d.extends, d.mixes_in
            )
        })
        .collect();
    println!("spec S6 declarations: {summary:#?}");
    let mixin = file
        .declarations
        .iter()
        .find(|d| d.name == "M")
        .expect("mixin M");
    assert!(
        mixin.mixes_in.is_empty(),
        "`mixin M on A` reports mixes_in={:?}: `on` is a superclass constraint, not a mixed-in type",
        mixin.mixes_in
    );
    let extension = file
        .declarations
        .iter()
        .find(|d| d.name == "X")
        .expect("extension X");
    assert!(
        extension.extends.is_none(),
        "extension `on` type leaks into the class-only `extends` field: {:?}",
        extension.extends
    );
}
