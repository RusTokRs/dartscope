use dartscope_core::{DartFileInput, DartIdentifierReferenceKind};
use dartscope_parse::analyze_file_with_references;

const SOURCE: &str = r#"
import 'remote.dart' as api;

class Service {
  static int count = 0;
  static int get status => count;
  static set status(int value) {}
  int value = 0;
  int get label => 'service';
  set label(String value) {}

  void exercise() {
    final first = this.value;
    this.value = 1;
    this.value += 2;
    final second = this.label;
    this.label = 'updated';
    Service.count++;
    Service.status = 3;
    final remote = api.Remote.flag;
    api.Remote.flag = true;
    Service.call();
  }
}
"#;

#[test]
fn emits_exact_property_declaration_and_access_facts() {
    let analysis = analyze_file_with_references(DartFileInput::new("lib/properties.dart", SOURCE));
    let properties = analysis
        .references
        .iter()
        .filter(|reference| is_property_kind(reference.kind))
        .collect::<Vec<_>>();

    let declarations = properties
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
                    | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 6);
    assert_eq!(
        declarations
            .iter()
            .map(|reference| (reference.name.as_str(), reference.kind))
            .collect::<Vec<_>>(),
        [
            (
                "count",
                DartIdentifierReferenceKind::MemberPropertyDeclarationStatic,
            ),
            (
                "status",
                DartIdentifierReferenceKind::MemberPropertyDeclarationStatic,
            ),
            (
                "status",
                DartIdentifierReferenceKind::MemberPropertyDeclarationStatic,
            ),
            (
                "value",
                DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
            ),
            (
                "label",
                DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
            ),
            (
                "label",
                DartIdentifierReferenceKind::MemberPropertyDeclarationInstance,
            ),
        ]
    );
    assert!(declarations.iter().all(|reference| {
        reference
            .prefix
            .as_deref()
            .is_some_and(|owner| owner.contains("Service"))
    }));

    let accesses = properties
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                DartIdentifierReferenceKind::MemberPropertyReadInstance
                    | DartIdentifierReferenceKind::MemberPropertyReadStatic
                    | DartIdentifierReferenceKind::MemberPropertyWriteInstance
                    | DartIdentifierReferenceKind::MemberPropertyWriteStatic
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(accesses.len(), 11);
    assert_eq!(
        accesses
            .iter()
            .filter(|reference| reference.name == "value")
            .map(|reference| reference.kind)
            .collect::<Vec<_>>(),
        [
            DartIdentifierReferenceKind::MemberPropertyReadInstance,
            DartIdentifierReferenceKind::MemberPropertyWriteInstance,
            DartIdentifierReferenceKind::MemberPropertyReadInstance,
            DartIdentifierReferenceKind::MemberPropertyWriteInstance,
        ]
    );
    assert_eq!(
        accesses
            .iter()
            .filter(|reference| reference.name == "flag")
            .map(|reference| (reference.kind, reference.prefix.as_deref()))
            .collect::<Vec<_>>(),
        [
            (
                DartIdentifierReferenceKind::MemberPropertyReadStatic,
                Some("api.Remote"),
            ),
            (
                DartIdentifierReferenceKind::MemberPropertyWriteStatic,
                Some("api.Remote"),
            ),
        ]
    );
    assert!(!properties.iter().any(|reference| reference.name == "call"));

    for reference in properties {
        assert_eq!(
            &SOURCE[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
    }
}

fn is_property_kind(kind: DartIdentifierReferenceKind) -> bool {
    matches!(
        kind,
        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance
            | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic
            | DartIdentifierReferenceKind::MemberPropertyReadInstance
            | DartIdentifierReferenceKind::MemberPropertyReadStatic
            | DartIdentifierReferenceKind::MemberPropertyWriteInstance
            | DartIdentifierReferenceKind::MemberPropertyWriteStatic
    )
}
