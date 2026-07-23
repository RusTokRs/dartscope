use crate::{analyze_file, analyze_file_with_references, analyze_project};
use dartscope_core::{DartFileInput, DartProjectInput};
use serde_json::Value;

const PATH: &str = "lib/worker.dart";

fn strip_location_identity(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object
                .retain(|key, _| key != "span" && !key.ends_with("_span") && !key.ends_with("_id"));
            for value in object.values_mut() {
                strip_location_identity(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_location_identity(value);
            }
        }
        _ => {}
    }
}

fn remove_noise_only_facts(value: &mut Value) {
    let file = value
        .as_object_mut()
        .expect("serialized file analysis must be an object");
    file.insert("string_constants".to_string(), Value::Array(Vec::new()));
    if let Some(Value::Array(declarations)) = file.get_mut("declarations") {
        declarations
            .retain(|declaration| declaration.get("name").and_then(Value::as_str) != Some("noise"));
    }
}

fn normalized_file_analysis(source: &str) -> Value {
    let analysis = analyze_file(DartFileInput::new(PATH, source));
    let mut value = serde_json::to_value(analysis).expect("file analysis must serialize");
    strip_location_identity(&mut value);
    value
}

fn normalized_reference_analysis(source: &str) -> Value {
    let analysis = analyze_file_with_references(DartFileInput::new(PATH, source));
    let mut value = serde_json::to_value(analysis).expect("reference analysis must serialize");
    strip_location_identity(&mut value);

    let analysis = value
        .as_object_mut()
        .expect("serialized reference analysis must be an object");
    remove_noise_only_facts(
        analysis
            .get_mut("file")
            .expect("reference analysis must contain file facts"),
    );
    for collection in ["references", "bindings"] {
        if let Some(Value::Array(items)) = analysis.get_mut(collection) {
            items.retain(|item| item.get("name").and_then(Value::as_str) != Some("noise"));
        }
    }
    value
}

#[test]
fn semantic_facts_are_stable_across_lf_and_crlf() {
    let lf = r#"import 'src/api.dart' show Api;
export 'src/public.dart' hide Internal;
part 'worker.g.dart';

class Worker {
  void run(List<int> values) {
    for (final value in values)
      if (value.isEven)
        consume(value);
      else
        try {
          consume(value);
        } on StateError catch (error) {
          report(error);
        } finally {
          cleanup();
        }
  }
}
"#;
    let crlf = lf.replace('\n', "\r\n");

    assert_eq!(
        normalized_file_analysis(lf),
        normalized_file_analysis(&crlf)
    );
}

#[test]
fn comments_and_raw_strings_do_not_inject_declarations_or_references() {
    let real_program = r#"class Worker {
  void run(List<int> values) {
    for (final value in values)
      if (value.isEven)
        consume(value);
      else
        try {
          consume(value);
        } on StateError catch (error) {
          report(error);
        } finally {
          cleanup();
        }
  }
}
"#;
    let noisy_program = format!(
        r#"const noise = r'''
class Fake {{
  void injected() {{
    for (final bogus in values) try {{ fake(); }} finally {{ nope(); }}
  }}
}}
''';
/* class Commented {{ void hidden() {{ phantom(); }} }} */
{real_program}"#
    );

    assert_eq!(
        normalized_reference_analysis(real_program),
        normalized_reference_analysis(&noisy_program)
    );
}

#[test]
fn project_analysis_reuses_the_same_file_semantics_as_single_file_analysis() {
    let source = r#"import 'src/api.dart';
class Worker {
  void run() {
    for (var index = 0; index < 3; index++) {
      consume(index);
    }
  }
}
"#;
    let individual = analyze_file(DartFileInput::new(PATH, source));
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(PATH, source)],
        Vec::new(),
    ));

    assert_eq!(project.files, vec![individual]);
}
