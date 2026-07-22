use dartscope_core::{
    DartFileInput, DartProjectInput, DiagnosticSeverity, PackageConfigInput, PubspecInput,
};
use dartscope_parse::{
    HeuristicDartParser, analyze_project, analyze_project_with_parser,
    analyze_project_with_references,
};

#[test]
fn identical_duplicate_inputs_are_deduplicated_and_reported() {
    let analysis = analyze_project(
        DartProjectInput::new(
            "demo",
            vec![
                DartFileInput::new("lib/main.dart", "class App {}"),
                DartFileInput::new("lib/main.dart", "class App {}"),
            ],
            vec![
                PubspecInput::new("pubspec.yaml", "name: demo"),
                PubspecInput::new("pubspec.yaml", "name: demo"),
            ],
        )
        .with_package_configs(vec![
            PackageConfigInput::new(
                ".dart_tool/package_config.json",
                r#"{"configVersion":2,"packages":[]}"#,
            ),
            PackageConfigInput::new(
                ".dart_tool/package_config.json",
                r#"{"configVersion":2,"packages":[]}"#,
            ),
        ]),
    );

    assert_eq!(analysis.summary.dart_files, 1);
    assert_eq!(analysis.summary.pubspecs, 1);
    assert_eq!(analysis.summary.package_configs, 1);

    let duplicates: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "duplicate_project_input_path")
        .collect();
    assert_eq!(duplicates.len(), 3);
    assert!(
        duplicates
            .iter()
            .all(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    );
}

#[test]
fn conflicting_normalized_paths_are_rejected_before_reference_analysis() {
    let analysis = analyze_project_with_references(DartProjectInput {
        root: "demo".to_string(),
        files: vec![
            DartFileInput {
                path: "lib\\main.dart".to_string(),
                source: "class First {}".to_string(),
            },
            DartFileInput {
                path: "lib/main.dart".to_string(),
                source: "class Second {}".to_string(),
            },
        ],
        pubspecs: Vec::new(),
        package_configs: Vec::new(),
    });

    assert!(analysis.project.files.is_empty());
    assert_eq!(analysis.project.summary.dart_files, 0);
    assert!(analysis.references.is_empty());
    assert!(analysis.bindings.is_empty());

    let diagnostic = analysis
        .project
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "duplicate_project_input_path")
        .expect("duplicate-path diagnostic");
    assert_eq!(diagnostic.path.as_deref(), Some("lib/main.dart"));
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
}

#[test]
fn custom_parser_entrypoint_uses_the_same_input_preflight() {
    let analysis = analyze_project_with_parser(
        &HeuristicDartParser,
        DartProjectInput::new(
            "demo",
            vec![
                DartFileInput::new("lib/main.dart", "class App {}"),
                DartFileInput::new("lib/main.dart", "class App {}"),
            ],
            Vec::new(),
        ),
    );

    assert_eq!(analysis.files.len(), 1);
    assert_eq!(analysis.summary.dart_files, 1);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "duplicate_project_input_path")
    );
}
