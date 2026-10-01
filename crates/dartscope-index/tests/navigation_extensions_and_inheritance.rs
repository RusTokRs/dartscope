//! Member navigation through inheritance and extensions.
//!
//! A member that is not declared by the type itself comes from its supertypes (superclass chain,
//! mixins, `on` constraints) or from an extension that is visible and applies to the receiver.
//! The first four specs came from the 2026-09-30 audit, which found extensions of unrelated types
//! used as definitions and inheritance followed one level only.

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
                "{}:{:?}:{} basis={:?} id={}",
                c.declaration_path,
                c.kind,
                c.name,
                c.basis,
                c.symbol_id.as_deref().unwrap_or("-")
            ),
            DartDefinitionTarget::Lexical(b) => format!("lexical {}", b.name),
        })
        .collect();
    (resolution.status, targets)
}

#[test]
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

fn assert_resolved_to(
    files: &[(&str, &str)],
    file: &str,
    fragment: &str,
    token: &str,
    expected_symbol_id: &str,
) -> Vec<String> {
    let (status, targets) = resolve(files, file, fragment, token);
    assert_eq!(
        status,
        DartDefinitionResolutionStatus::Resolved,
        "{fragment}: {targets:?}"
    );
    assert_eq!(targets.len(), 1, "{fragment}: {targets:?}");
    assert!(
        targets[0].ends_with(&format!("id={expected_symbol_id}")),
        "{fragment}: expected {expected_symbol_id}, got {targets:?}"
    );
    targets
}

#[test]
fn the_nearest_declaration_wins_over_one_further_up() {
    let files = [(
        "lib/a.dart",
        "class A {\n  void f() {}\n}\nclass B extends A {\n  void f() {}\n}\nclass C extends B {\n  void go() { this.f(); }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.f",
        "f",
        "lib/a.dart::class:B/method:f",
    );
}

#[test]
fn a_mixin_member_takes_precedence_over_the_superclass_member() {
    let files = [(
        "lib/a.dart",
        "class B {\n  void f() {}\n}\nmixin M {\n  void f() {}\n}\nclass C extends B with M {\n  void go() { this.f(); }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.f",
        "f",
        "lib/a.dart::mixin:M/method:f",
    );
}

#[test]
fn the_last_applied_mixin_takes_precedence() {
    let files = [(
        "lib/a.dart",
        "mixin M1 {\n  void f() {}\n}\nmixin M2 {\n  void f() {}\n}\nclass C with M1, M2 {\n  void go() { this.f(); }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.f",
        "f",
        "lib/a.dart::mixin:M2/method:f",
    );
}

#[test]
fn a_member_reached_through_a_mixin_and_a_superclass_chain_is_resolved() {
    let files = [(
        "lib/a.dart",
        "class A {\n  void fromA() {}\n}\nclass B extends A {}\nmixin M {\n  void fromM() {}\n}\nclass C extends B with M {\n  void go() {\n    this.fromA();\n    this.fromM();\n  }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.fromA",
        "fromA",
        "lib/a.dart::class:A/method:fromA",
    );
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.fromM",
        "fromM",
        "lib/a.dart::mixin:M/method:fromM",
    );
}

#[test]
fn an_inheritance_cycle_ends_without_a_definition() {
    let files = [(
        "lib/a.dart",
        "class A extends B {\n  void go() { this.missing(); }\n}\nclass B extends A {}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.missing", "missing");
    assert_eq!(status, DartDefinitionResolutionStatus::Missing, "{targets:?}");
}

#[test]
fn a_mixin_sees_the_members_of_its_on_constraint() {
    let files = [(
        "lib/a.dart",
        "class A {\n  void fromA() {}\n}\nmixin M on A {\n  void go() { this.fromA(); }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.fromA",
        "fromA",
        "lib/a.dart::class:A/method:fromA",
    );
}

#[test]
fn an_extension_body_sees_the_members_of_the_extended_type() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void bar() {}\n}\nextension FooX on Foo {\n  void go() { this.bar(); }\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.bar",
        "bar",
        "lib/a.dart::class:Foo/method:bar",
    );
}

#[test]
fn an_extension_of_the_receiver_type_applies() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void run() { this.zap(); }\n}\nextension FooX on Foo {\n  int zap() => 1;\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.zap",
        "zap",
        "lib/a.dart::extension:FooX/method:zap",
    );
}

#[test]
fn an_extension_of_a_supertype_applies_to_its_subtypes() {
    let files = [(
        "lib/a.dart",
        "class Base {}\nclass Sub extends Base {\n  void run() { this.zap(); }\n}\nextension BaseX on Base {\n  int zap() => 1;\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.zap",
        "zap",
        "lib/a.dart::extension:BaseX/method:zap",
    );
}

#[test]
fn an_extension_of_a_generic_type_applies_by_its_name() {
    let files = [(
        "lib/a.dart",
        "class Box<T> {\n  void run() { this.zap(); }\n}\nextension BoxX<T> on Box<T> {\n  int zap() => 1;\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.zap",
        "zap",
        "lib/a.dart::extension:BoxX/method:zap",
    );
}

#[test]
fn an_extension_of_object_or_of_its_own_type_parameter_applies_to_everything() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void run() {\n    this.onObject();\n    this.onAnything();\n  }\n}\nextension ObjectX on Object {\n  int onObject() => 1;\n}\nextension AnyX<T> on T {\n  int onAnything() => 1;\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.onObject",
        "onObject",
        "lib/a.dart::extension:ObjectX/method:onObject",
    );
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.onAnything",
        "onAnything",
        "lib/a.dart::extension:AnyX/method:onAnything",
    );
}

#[test]
fn an_extension_type_member_is_never_an_extension_member_of_other_types() {
    let files = [(
        "lib/a.dart",
        "extension type Id(int value) {\n  int zap() => 1;\n}\nclass Foo {\n  void run() { this.zap(); }\n}\n",
    )];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.zap", "zap");
    assert_ne!(status, DartDefinitionResolutionStatus::Resolved, "{targets:?}");
}

#[test]
fn an_imported_extension_applies_with_the_basis_of_its_import() {
    let files = [
        (
            "lib/ext.dart",
            "class Foo {}\nextension FooX on Foo {\n  void extra() {}\n}\n",
        ),
        (
            "lib/a.dart",
            "import 'ext.dart';\nclass Bar extends Foo {\n  void go() { this.extra(); }\n}\n",
        ),
    ];
    let targets = assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.extra",
        "extra",
        "lib/ext.dart::extension:FooX/method:extra",
    );
    assert!(targets[0].contains("basis=DirectImport"), "{targets:?}");
}

#[test]
fn an_extension_imported_with_a_prefix_still_applies() {
    // Dart applies extensions of prefixed imports implicitly too.
    let files = [
        (
            "lib/ext.dart",
            "class Foo {}\nextension FooX on Foo {\n  void extra() {}\n}\n",
        ),
        (
            "lib/a.dart",
            "import 'ext.dart' as e;\nclass Bar extends e.Foo {\n  void go() { this.extra(); }\n}\n",
        ),
    ];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.extra",
        "extra",
        "lib/ext.dart::extension:FooX/method:extra",
    );
}

#[test]
fn an_extension_hidden_by_the_import_does_not_apply() {
    let files = [
        (
            "lib/ext.dart",
            "class Foo {}\nextension FooX on Foo {\n  void extra() {}\n}\n",
        ),
        (
            "lib/a.dart",
            "import 'ext.dart' hide FooX;\nclass Bar extends Foo {\n  void go() { this.extra(); }\n}\n",
        ),
    ];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.extra", "extra");
    assert_ne!(status, DartDefinitionResolutionStatus::Resolved, "{targets:?}");
}

#[test]
fn an_unnamed_extension_is_not_visible_outside_its_library() {
    let files = [
        (
            "lib/ext.dart",
            "class Foo {}\nextension on Foo {\n  void extra() {}\n}\n",
        ),
        (
            "lib/a.dart",
            "import 'ext.dart';\nclass Bar extends Foo {\n  void go() { this.extra(); }\n}\n",
        ),
    ];
    let (status, targets) = resolve(&files, "lib/a.dart", "this.extra", "extra");
    assert_ne!(status, DartDefinitionResolutionStatus::Resolved, "{targets:?}");
}

#[test]
fn an_unnamed_extension_applies_inside_its_own_library() {
    let files = [(
        "lib/a.dart",
        "class Foo {\n  void run() { this.extra(); }\n}\nextension on Foo {\n  void extra() {}\n}\n",
    )];
    assert_resolved_to(
        &files,
        "lib/a.dart",
        "this.extra",
        "extra",
        "lib/a.dart::extension:/method:extra",
    );
}
