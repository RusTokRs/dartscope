use dartscope_core::{DartFileInput, DartStringConstant};
use dartscope_parse::analyze_file;

fn constants(source: &str) -> Vec<DartStringConstant> {
    analyze_file(DartFileInput::new("lib/sample.dart", source)).string_constants
}

fn value_of(source: &str, name: &str) -> Option<String> {
    constants(source)
        .into_iter()
        .find(|constant| constant.name == name)
        .map(|constant| constant.value)
}

#[test]
fn literal_forms_are_recorded_without_truncation() {
    let source = "\
const single = 'single';\n\
const double = \"double\";\n\
const raw = r'raw\\value';\n\
const triple = '''triple'value''';\n\
const applied = 'part one' ' part two';\n\
const escaped = 'it\\'s';\n";

    assert_eq!(value_of(source, "single").as_deref(), Some("single"));
    assert_eq!(value_of(source, "double").as_deref(), Some("double"));
    assert_eq!(value_of(source, "raw").as_deref(), Some("raw\\value"));
    assert_eq!(value_of(source, "triple").as_deref(), Some("triple'value"));
    assert_eq!(
        value_of(source, "applied").as_deref(),
        Some("part one part two")
    );
    assert_eq!(value_of(source, "escaped").as_deref(), Some("it\\'s"));
}

#[test]
fn multi_line_literal_value_and_span_cover_the_whole_literal() {
    let source = "const query = r'''\nquery Home {\n  viewer {\n    id\n  }\n}\n''';\n";
    let constant = constants(source)
        .into_iter()
        .find(|constant| constant.name == "query")
        .expect("query constant");

    assert_eq!(
        constant.value,
        "\nquery Home {\n  viewer {\n    id\n  }\n}\n"
    );
    assert_eq!(constant.span.start_line, 1);
    assert_eq!(constant.span.end_line, 7);
    assert_eq!(
        &source[constant.span.byte_start..constant.span.byte_end],
        "r'''\nquery Home {\n  viewer {\n    id\n  }\n}\n'''"
    );
}

#[test]
fn non_literal_initializers_are_not_string_constants() {
    let source = "\
final fromCall = readString('key');\n\
final numbers = <int>[1, 2];\n\
final number = 1;\n\
const collected = <String>[];\n";

    assert!(
        constants(source).is_empty(),
        "only literal initializers are string constants: {:?}",
        constants(source)
            .iter()
            .map(|constant| constant.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn computed_adjacent_literals_follow_dart_concatenation() {
    let source = "\
const route = '/modules'\n    '/:id';\n";

    assert_eq!(value_of(source, "route").as_deref(), Some("/modules/:id"));
}

#[test]
fn unterminated_literal_is_not_reported_as_a_constant() {
    let analysis = analyze_file(DartFileInput::new(
        "lib/sample.dart",
        "const broken = 'oops;\n",
    ));

    assert!(analysis.string_constants.is_empty());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unterminated_string")
    );
}

#[test]
fn directive_uris_share_the_literal_rules() {
    let analysis = analyze_file(DartFileInput::new(
        "lib/sample.dart",
        "import r'package:foo/foo.dart' as foo;\npart 'sample\\'s.g.dart';\n",
    ));

    assert_eq!(analysis.imports.len(), 1);
    assert_eq!(analysis.imports[0].uri, "package:foo/foo.dart");
    assert_eq!(analysis.imports[0].prefix.as_deref(), Some("foo"));
    assert_eq!(analysis.parts.len(), 1);
    // Literal content is reported as written, so the escaped quote is retained instead of truncating
    // the URI at the backslash.
    assert_eq!(analysis.parts[0].uri, "sample\\'s.g.dart");
}
