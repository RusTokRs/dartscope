use dartscope_core::{Confidence, DartFileInput, DartProjectInput};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

#[test]
fn extracts_bounded_invocation_target_references_with_exact_spans() {
    let source = r#"
import 'api.dart' as api;

void run() {
  api.load();
  Factory.create().build();
  client.query();
  // Ignored.call();
  final text = 'Hidden.call()';
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/client.dart", source));

    assert_eq!(analysis.references.len(), 3);
    assert_eq!(analysis.references[0].name, "load");
    assert_eq!(analysis.references[0].prefix.as_deref(), Some("api"));
    assert_eq!(analysis.references[0].confidence, Confidence::High);
    assert_eq!(analysis.references[1].name, "Factory");
    assert_eq!(analysis.references[1].prefix, None);
    assert_eq!(analysis.references[1].confidence, Confidence::Medium);
    assert_eq!(analysis.references[2].name, "client");

    for reference in &analysis.references {
        assert_eq!(
            &source[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
        assert!(
            reference
                .enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| symbol_id.ends_with("::function:run"))
        );
    }
}

#[test]
fn suppresses_parameters_visible_locals_and_members_before_namespace_resolution() {
    let source = r#"
import 'api.dart' as api;

void local() {}
void external() {}

class Controller {
  void member() {}

  void run(dynamic scoped, dynamic api) {
    scoped();
    api.load();
    member();
    {
      final local = callback;
      local();
    }
    local();
    external();
  }
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/scopes.dart", source));

    assert_eq!(
        analysis
            .references
            .iter()
            .map(|reference| (reference.name.as_str(), reference.prefix.as_deref()))
            .collect::<Vec<_>>(),
        [("local", None), ("external", None)]
    );
    for reference in &analysis.references {
        assert_eq!(
            &source[reference.span.byte_start..reference.span.byte_end],
            reference.name
        );
        assert!(
            reference
                .enclosing_symbol_id
                .as_deref()
                .is_some_and(|symbol_id| symbol_id.ends_with("/method:run"))
        );
    }
}

#[test]
fn local_declarations_do_not_retroactively_shadow_earlier_invocations() {
    let source = r#"
void target() {}

void run() {
  target();
  final target = callback;
  target();
}
"#;
    let analysis = analyze_file_with_references(DartFileInput::new("lib/order.dart", source));

    assert_eq!(analysis.references.len(), 1);
    assert_eq!(analysis.references[0].name, "target");
    assert_eq!(
        analysis.references[0].span.byte_start,
        source.find("target();").expect("first invocation")
    );
}

#[test]
fn project_reference_output_is_sorted_by_path_and_source_span() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/z.dart", "void z() { Zed(); }\n"),
            DartFileInput::new("lib/a.dart", "void a() { Alpha(); }\n"),
        ],
        vec![],
    ));

    assert_eq!(
        analysis
            .references
            .iter()
            .map(|reference| (reference.source_path.as_str(), reference.name.as_str()))
            .collect::<Vec<_>>(),
        [("lib/a.dart", "Alpha"), ("lib/z.dart", "Zed")]
    );
}
