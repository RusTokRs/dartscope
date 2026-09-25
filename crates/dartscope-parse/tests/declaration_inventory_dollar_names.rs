//! Real-world Dart identifier shapes: `$` names and unnamed extensions.
//!
//! Dart identifiers accept `$` anywhere after the first character, and generated or
//! framework-facing sources rely on it (`_$UserFromJson`, `UrlRequestCallbackProxy$Interface`,
//! `jni$_`). An unnamed `extension on T { ... }` has no declarable name, so the inventory reports the
//! declaration with an empty name and keeps its members instead of dropping the body.

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
                "missing {kind:?} declaration for {name:?}; found {:?}",
                analysis
                    .declarations
                    .iter()
                    .map(|declaration| (declaration.name.as_str(), declaration.kind))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn dollar_names_keep_every_character_of_the_declared_name() {
    let source = "\
class Widget$Base {
  int count$ = 0;
  int get value$ => count$;
  set value$(int next) {
    count$ = next;
  }

  void render$() {}
}

Widget$Base? lookup$() => null;

void use() {
  final Widget$Base base = Widget$Base();
  base.render$();
}

final _$jniVersionCheck = 1;
";
    let analysis = declarations(source);

    let owner = named(&analysis, "Widget$Base", DartDeclarationKind::Class);
    assert_eq!(
        owner.symbol_id.as_deref(),
        Some("lib/sample.dart::class:Widget$Base")
    );
    for (name, kind) in [
        ("count$", DartDeclarationKind::Field),
        ("value$", DartDeclarationKind::Getter),
        ("value$", DartDeclarationKind::Setter),
        ("render$", DartDeclarationKind::Method),
    ] {
        assert_eq!(
            named(&analysis, name, kind).parent_symbol_id.as_deref(),
            owner.symbol_id.as_deref()
        );
    }
    named(&analysis, "lookup$", DartDeclarationKind::Function);
    named(
        &analysis,
        "_$jniVersionCheck",
        DartDeclarationKind::Variable,
    );
    let local = named(&analysis, "base", DartDeclarationKind::LocalVariable);
    assert_eq!(
        local.parent_symbol_id.as_deref(),
        named(&analysis, "use", DartDeclarationKind::Function)
            .symbol_id
            .as_deref()
    );

    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn dollar_names_are_not_merged_with_their_truncated_prefix() {
    let source = "\
void _$firstFromJson() {}

void _$secondFromJson() {}

void use() {
  _$firstFromJson();
  _$secondFromJson();
}
";
    let analysis = declarations(source);

    let first = named(&analysis, "_$firstFromJson", DartDeclarationKind::Function);
    let second = named(&analysis, "_$secondFromJson", DartDeclarationKind::Function);
    assert_ne!(first.symbol_id, second.symbol_id);
    assert_eq!(
        first.symbol_id.as_deref(),
        Some("lib/sample.dart::function:_$firstFromJson")
    );
}

#[test]
fn extension_type_name_with_dollar_sign_is_complete() {
    let source = "\
extension type const UrlRequestCallbackProxy$Interface._(Object value) {}

class _$Generated {
  static const String marker = 'value';
}

enum Outer$Inner { one }
";
    let analysis = declarations(source);

    named(
        &analysis,
        "UrlRequestCallbackProxy$Interface",
        DartDeclarationKind::ExtensionType,
    );
    named(&analysis, "_$Generated", DartDeclarationKind::Class);
    named(&analysis, "Outer$Inner", DartDeclarationKind::Enum);
}

#[test]
fn unnamed_extension_is_reported_with_its_members() {
    let source = "\
extension on List<int> {
  int sum$() => fold(0, (int a, int b) => a + b);
}

extension Named on List<int> {
  int total() => length;
}
";
    let analysis = declarations(source);

    let unnamed = named(&analysis, "", DartDeclarationKind::Extension);
    assert_eq!(
        unnamed.symbol_id.as_deref(),
        Some("lib/sample.dart::extension:")
    );
    assert_eq!(unnamed.span.start_line, 1);
    assert_eq!(
        unnamed.declaration_span.as_ref().map(|span| span.end_line),
        Some(3)
    );

    let member = named(&analysis, "sum$", DartDeclarationKind::Method);
    assert_eq!(
        member.parent_symbol_id.as_deref(),
        unnamed.symbol_id.as_deref()
    );
    assert_eq!(
        member.symbol_id.as_deref(),
        Some("lib/sample.dart::extension:/method:sum$")
    );

    let named_extension = named(&analysis, "Named", DartDeclarationKind::Extension);
    assert_eq!(
        named_extension.symbol_id.as_deref(),
        Some("lib/sample.dart::extension:Named")
    );
    let total = named(&analysis, "total", DartDeclarationKind::Method);
    assert_eq!(
        total.parent_symbol_id.as_deref(),
        named_extension.symbol_id.as_deref()
    );
}

#[test]
fn repeated_unnamed_extensions_receive_distinct_symbol_ids() {
    let source = "\
extension on List<int> {
  int first$() => 1;
}

extension on List<String> {
  int second$() => 2;
}
";
    let analysis = declarations(source);

    let unnamed: Vec<_> = analysis
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.kind == DartDeclarationKind::Extension && declaration.name.is_empty()
        })
        .collect();
    assert_eq!(unnamed.len(), 2);
    assert_eq!(
        unnamed[0].symbol_id.as_deref(),
        Some("lib/sample.dart::extension:")
    );
    assert_eq!(
        unnamed[1].symbol_id.as_deref(),
        Some("lib/sample.dart::extension:#2")
    );
    assert_eq!(
        named(&analysis, "first$", DartDeclarationKind::Method)
            .parent_symbol_id
            .as_deref(),
        unnamed[0].symbol_id.as_deref()
    );
    assert_eq!(
        named(&analysis, "second$", DartDeclarationKind::Method)
            .parent_symbol_id
            .as_deref(),
        unnamed[1].symbol_id.as_deref()
    );
}

#[test]
fn multi_line_unnamed_extension_header_is_reported() {
    let source = "extension on\n    Map<String, int> {\n  int get total$ => length;\n}\n";
    let analysis = declarations(source);

    let unnamed = named(&analysis, "", DartDeclarationKind::Extension);
    assert_eq!(unnamed.span.start_line, 1);
    assert_eq!(
        named(&analysis, "total$", DartDeclarationKind::Getter)
            .parent_symbol_id
            .as_deref(),
        unnamed.symbol_id.as_deref()
    );
}
