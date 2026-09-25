//! Namespace resolution for `$` identifiers.
//!
//! Import prefixes, combinators, and prefixed references all travel through the same Dart identifier
//! rules as declarations. A `$` in any of those positions must resolve exactly, otherwise go-to
//! definition on generated code such as `bindings$.Widget$Base()` silently fails.

use dartscope_core::{
    DartDeclarationKind, DartFileInput, DartProjectInput, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus,
};
use dartscope_index::resolve_project_identifier_references;
use dartscope_parse::analyze_project_with_references;

const BINDINGS: &str = r#"
class Widget$Base {
  int count$ = 0;

  void render$() {}
}

void register$() {}

void _$internal() {}

class Excluded$Type {}
"#;

const CLIENT: &str = r#"
import 'bindings.dart' as bindings$ show Widget$Base, register$, _$internal;

void run() {
  bindings$.Widget$Base();
  bindings$.register$();
  bindings$._$internal();
  bindings$.Excluded$Type();
}
"#;

fn client_resolutions() -> Vec<dartscope_core::DartIdentifierReferenceResolution> {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/bindings.dart", BINDINGS),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    resolve_project_identifier_references(&analysis).resolutions
}

fn resolution_for<'analysis>(
    resolutions: &'analysis [dartscope_core::DartIdentifierReferenceResolution],
    name: &str,
) -> &'analysis dartscope_core::DartIdentifierReferenceResolution {
    resolutions
        .iter()
        .find(|resolution| resolution.reference.name == name)
        .unwrap_or_else(|| {
            panic!(
                "missing resolution for {name:?}; found {:?}",
                resolutions
                    .iter()
                    .map(|resolution| resolution.reference.name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn prefixed_references_to_dollar_names_resolve_to_their_declarations() {
    let resolutions = client_resolutions();
    assert_eq!(resolutions.len(), 4);

    for (name, kind, symbol_id) in [
        (
            "Widget$Base",
            DartDeclarationKind::Class,
            "lib/bindings.dart::class:Widget$Base",
        ),
        (
            "register$",
            DartDeclarationKind::Function,
            "lib/bindings.dart::function:register$",
        ),
    ] {
        let resolution = resolution_for(&resolutions, name);
        assert_eq!(resolution.reference.prefix.as_deref(), Some("bindings$"));
        assert_eq!(resolution.status, DartSymbolResolutionStatus::Resolved);
        let candidate = resolution.candidates.first().expect("resolved candidate");
        assert_eq!(candidate.name, name);
        assert_eq!(candidate.kind, kind);
        assert_eq!(candidate.symbol_id.as_deref(), Some(symbol_id));
        assert_eq!(candidate.basis, DartSymbolResolutionBasis::DirectImport);
    }
}

#[test]
fn dollar_names_keep_library_privacy_and_combinator_filtering() {
    let resolutions = client_resolutions();

    let private = resolution_for(&resolutions, "_$internal");
    assert_eq!(private.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(
        private.candidates.first().map(|candidate| candidate.basis),
        Some(DartSymbolResolutionBasis::NotVisible),
        "a leading underscore stays library-private even when the name contains a dollar sign"
    );

    let hidden = resolutions
        .iter()
        .find(|resolution| resolution.reference.name == "Excluded$Type")
        .expect("excluded reference");
    assert_eq!(hidden.status, DartSymbolResolutionStatus::NotVisible);
    assert_eq!(
        hidden.candidates.first().map(|candidate| candidate.basis),
        Some(DartSymbolResolutionBasis::NotVisible)
    );
}
