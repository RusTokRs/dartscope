use dartscope_core::{
    DartFileAnalysis, DartFileInput, PackageConfigInput, SourceSpan, normalize_path,
};
use dartscope_parse::analyze_file;
use dartscope_resolve::{PackageUriResolutionError, parse_package_config, resolve_package_uri};

const PACKAGE_CONFIG: &str = r#"{
  "configVersion": 2,
  "packages": [
    {
      "name": "app",
      "rootUri": "../",
      "packageUri": "lib/",
      "languageVersion": "3.13"
    }
  ]
}"#;

#[test]
fn generated_path_normalization_is_idempotent_and_constructor_stable() {
    for seed in 0..256_u64 {
        let raw = generated_path(seed);
        let normalized = normalize_path(raw.clone());

        assert_eq!(
            normalize_path(normalized.clone()),
            normalized,
            "seed {seed}"
        );
        assert_eq!(normalized, raw.replace('\\', "/"), "seed {seed}");
        assert!(!normalized.contains('\\'), "seed {seed}");

        let input = DartFileInput::new(raw, "");
        assert_eq!(input.path, normalized, "seed {seed}");
    }
}

#[test]
fn generated_analysis_has_exact_monotonic_spans_and_stable_ordering() {
    for seed in 0..32_u64 {
        for line_ending in ["\n", "\r\n"] {
            let source = generated_source(seed, line_ending);
            let input = DartFileInput::new(format!(r"lib\generated_{seed}.dart"), source.clone());
            let first = analyze_file(input.clone());
            let second = analyze_file(input);

            assert_eq!(first, second, "analysis changed for seed {seed:?}");
            assert_eq!(first.path, format!("lib/generated_{seed}.dart"));
            assert_analysis_spans(&source, &first, seed, line_ending);
        }
    }
}

#[test]
fn generated_package_uris_are_deterministic_normalized_and_root_bounded() {
    let config = parse_package_config(PackageConfigInput::new(
        r"workspace\.dart_tool\package_config.json",
        PACKAGE_CONFIG,
    ));
    assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
    assert_eq!(config.path, "workspace/.dart_tool/package_config.json");

    for seed in 0..128_u64 {
        let library_path = generated_library_path(seed);
        let package_uri = format!("package:app/{library_path}");
        let first = resolve_package_uri(&config, &package_uri).expect("generated package URI");
        let second = resolve_package_uri(&config, &package_uri).expect("repeat resolution");
        let expected_path = format!("workspace/lib/{library_path}");

        assert_eq!(first, second, "resolution changed for {package_uri}");
        assert_eq!(first.package_name, "app");
        assert_eq!(first.project_path.as_deref(), Some(expected_path.as_str()));
        let project_path = first.project_path.as_deref().expect("project path");
        assert_eq!(normalize_path(project_path.to_string()), project_path);
        assert!(!project_path.contains('\\'));
        let expected_suffix = format!("/workspace/lib/{library_path}");
        assert!(
            first.resolved_uri.ends_with(&expected_suffix),
            "{}",
            first.resolved_uri
        );
    }

    for (uri, expected_path) in [
        ("package:app/./src/main.dart", "workspace/lib/src/main.dart"),
        (
            "package:app/src/generated/../main.dart",
            "workspace/lib/src/main.dart",
        ),
        (
            "package:app/space%20name.dart",
            "workspace/lib/space name.dart",
        ),
    ] {
        let resolved = resolve_package_uri(&config, uri).expect(uri);
        assert_eq!(
            resolved.project_path.as_deref(),
            Some(expected_path),
            "{uri}"
        );
    }

    for invalid in [
        "app/main.dart",
        "package:app/../secret.dart",
        "package:app/%2e%2e/secret.dart",
        "package:app/src%2fsecret.dart",
        "package:app/src%5csecret.dart",
        "package:app/src/main.dart?mode=test",
        "package:app/src/main.dart#fragment",
        "package:/main.dart",
    ] {
        assert!(
            matches!(
                resolve_package_uri(&config, invalid),
                Err(PackageUriResolutionError::InvalidPackageUri(_))
            ),
            "{invalid}"
        );
    }
}

fn assert_analysis_spans(source: &str, analysis: &DartFileAnalysis, seed: u64, line_ending: &str) {
    let case = format!(
        "seed {seed}, {}",
        if line_ending == "\n" { "LF" } else { "CRLF" }
    );

    if let Some(library) = analysis.library.as_ref() {
        assert_span(source, &library.span, "library", &case);
    }
    assert_monotonic(
        source,
        "imports",
        analysis.imports.iter().map(|item| &item.span),
        &case,
    );
    assert_monotonic(
        source,
        "exports",
        analysis.exports.iter().map(|item| &item.span),
        &case,
    );
    assert_monotonic(
        source,
        "parts",
        analysis.parts.iter().map(|item| &item.span),
        &case,
    );
    if let Some(part_of) = analysis.part_of.as_ref() {
        assert_span(source, &part_of.span, "part_of", &case);
    }

    assert_monotonic(
        source,
        "declarations",
        analysis.declarations.iter().map(|item| &item.span),
        &case,
    );
    for declaration in &analysis.declarations {
        if let Some(span) = declaration.declaration_span.as_ref() {
            assert_span(source, span, "declaration extent", &case);
        }
    }

    assert_monotonic(
        source,
        "string_constants",
        analysis.string_constants.iter().map(|item| &item.span),
        &case,
    );
    assert_monotonic(
        source,
        "graphql_operations",
        analysis.graphql_operations.iter().map(|item| &item.span),
        &case,
    );
    assert_monotonic(
        source,
        "graphql_operation_uses",
        analysis
            .graphql_operation_uses
            .iter()
            .map(|item| &item.span),
        &case,
    );

    assert_monotonic(
        source,
        "invocations",
        analysis.invocations.iter().map(|item| &item.span),
        &case,
    );
    for invocation in &analysis.invocations {
        assert_span(
            source,
            &invocation.source_line_span,
            "invocation source line",
            &case,
        );
        assert_monotonic(
            source,
            "invocation arguments",
            invocation.arguments.iter().map(|argument| &argument.span),
            &case,
        );
        for argument in &invocation.arguments {
            assert_monotonic(
                source,
                "map entries",
                argument.map_entries.iter().map(|entry| &entry.span),
                &case,
            );
            for entry in &argument.map_entries {
                assert_span(
                    source,
                    &entry.source_line_span,
                    "map entry source line",
                    &case,
                );
            }
        }
    }

    for span in analysis
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.span.as_ref())
    {
        assert_span(source, span, "diagnostic", &case);
    }
}

fn assert_monotonic<'a>(
    source: &str,
    label: &str,
    spans: impl IntoIterator<Item = &'a SourceSpan>,
    case: &str,
) {
    let mut previous = None;
    for span in spans {
        assert_span(source, span, label, case);
        if let Some(previous_start) = previous {
            assert!(
                previous_start <= span.byte_start,
                "{label} spans are not monotonic for {case}: {previous_start} > {}",
                span.byte_start
            );
        }
        previous = Some(span.byte_start);
    }
}

fn assert_span(source: &str, span: &SourceSpan, label: &str, case: &str) {
    assert!(
        span.byte_start <= span.byte_end,
        "{label} reversed for {case}: {span:?}"
    );
    assert!(
        span.byte_end <= source.len(),
        "{label} out of bounds for {case}: {span:?}, len {}",
        source.len()
    );
    assert!(
        source.is_char_boundary(span.byte_start) && source.is_char_boundary(span.byte_end),
        "{label} splits UTF-8 for {case}: {span:?}"
    );

    let (start_line, start_column) = position(source, span.byte_start);
    let (end_line, end_column) = position(source, span.byte_end);
    assert_eq!(
        (span.start_line, span.start_column),
        (start_line, start_column),
        "{label} start coordinates for {case}: {span:?}"
    );
    assert_eq!(
        (span.end_line, span.end_column),
        (end_line, end_column),
        "{label} end coordinates for {case}: {span:?}"
    );
}

fn position(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..byte_offset].chars().count() + 1;
    (line, column)
}

fn generated_source(seed: u64, line_ending: &str) -> String {
    let class_name = format!("Box{seed}");
    let source = [
        format!("// deterministic λ seed {seed}"),
        "library generated.properties;".to_string(),
        "import 'package:demo/api.dart' show Api;".to_string(),
        "export 'src/model.dart' hide Internal;".to_string(),
        "part 'src/generated_part.dart';".to_string(),
        format!(
            "const queryDoc = r'''query Fetch{seed}($id: ID!) {{ node(id: $id) {{ id }} }}''';"
        ),
        format!("class {class_name} {{"),
        format!("  final String label = 'λ-{seed}';"),
        format!("  {class_name}();"),
        "  void run() {".to_string(),
        format!(
            "    final local = Api.load(path: 'src/{seed}.dart', options: {{'key': 'value'}});"
        ),
        "    client.query(document: queryDoc, variables: {'id': 'λ-value'});".to_string(),
        "    print(local);".to_string(),
        "  }".to_string(),
        "}".to_string(),
    ]
    .join(line_ending);
    format!("{source}{line_ending}")
}

fn generated_path(seed: u64) -> String {
    const ALPHABET: &[char] = &['a', 'b', 'Z', '0', '_', '-', '.', '/', '\\', 'λ', '中', ' '];
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let length = (next(&mut state) % 48 + 1) as usize;
    let mut path = String::with_capacity(length);
    for _ in 0..length {
        let index = (next(&mut state) as usize) % ALPHABET.len();
        path.push(ALPHABET[index]);
    }
    path
}

fn generated_library_path(seed: u64) -> String {
    let depth = (seed % 4 + 1) as usize;
    let mut segments = (0..depth)
        .map(|index| format!("segment_{seed}_{index}"))
        .collect::<Vec<_>>();
    segments.push(format!("file_{seed}.dart"));
    segments.join("/")
}

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}
