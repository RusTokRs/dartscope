use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_lints::{
    DartForbiddenImportPattern, DartImportPatternKind, DartLayerBoundary, DartLintConfig,
    DartLintExclusions, DartLintPathMatch, DartLintRuleId, lint_project,
};
use dartscope_parse::analyze_project;

fn project(files: &[(&str, &str)]) -> dartscope_core::DartProjectAnalysis {
    analyze_project(DartProjectInput::new(
        ".",
        files
            .iter()
            .map(|(path, source)| DartFileInput::new(*path, *source))
            .collect(),
        vec![],
    ))
}

fn paths(analysis: &dartscope_lints::DartLintAnalysis) -> Vec<&str> {
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.path.as_str())
        .collect()
}

#[test]
fn excluded_suffixes_and_prefixes_silence_every_rule_for_those_files() {
    let project = project(&[
        ("lib/BadName.dart", "class bad_type {}\n"),
        ("lib/model.g.dart", "class bad_generated {}\n"),
        ("lib/generated/other.dart", "class bad_other {}\n"),
    ]);
    let mut config = DartLintConfig::new([DartLintRuleId::NamingConvention]);

    // Without exclusions every file is reported.
    let everything = lint_project(&project, &config);
    assert!(paths(&everything).contains(&"lib/model.g.dart"));
    assert!(paths(&everything).contains(&"lib/generated/other.dart"));

    config.exclude = DartLintExclusions {
        path_prefixes: vec!["lib\\generated\\".to_string()],
        path_suffixes: vec![".g.dart".to_string()],
    };
    let filtered = lint_project(&project, &config);

    assert!(!filtered.diagnostics.is_empty());
    assert!(paths(&filtered).iter().all(|path| *path == "lib/BadName.dart"));
    assert_eq!(filtered.summary.diagnostics, filtered.diagnostics.len());
}

#[test]
fn segment_matching_does_not_cross_a_directory_name() {
    let project = project(&[
        ("lib/ui/screen.dart", "import '../database/store.dart';\nimport '../data/repo.dart';\n"),
        ("lib/ui_kit/button.dart", "import '../data/repo.dart';\n"),
        ("lib/data/repo.dart", "class Repo {}\n"),
        ("lib/database/store.dart", "class Store {}\n"),
    ]);
    let boundary = DartLayerBoundary {
        source_prefix: "lib/ui".to_string(),
        denied_target_prefixes: vec!["lib/data".to_string()],
    };

    // A plain string prefix: `lib/ui` covers `lib/ui_kit/`, and `lib/data` covers `lib/database/`.
    let mut config = DartLintConfig::new([DartLintRuleId::LayerBoundary]);
    config.layer_boundaries.push(boundary.clone());
    let by_string = lint_project(&project, &config);
    assert_eq!(
        paths(&by_string),
        ["lib/ui/screen.dart", "lib/ui/screen.dart", "lib/ui_kit/button.dart"]
    );

    // Segments: only `lib/ui/` is the source layer, and only `lib/data/` is denied.
    config.path_match = DartLintPathMatch::Segment;
    let by_segment = lint_project(&project, &config);
    assert_eq!(paths(&by_segment), ["lib/ui/screen.dart"]);
    assert_eq!(
        by_segment.diagnostics[0].related_paths,
        ["lib/data/repo.dart"]
    );
}

#[test]
fn segment_matching_applies_to_ignored_paths_and_exclusions() {
    let project = project(&[
        ("lib/gen/a.dart", "class bad_a {}\n"),
        ("lib/generic/b.dart", "class bad_b {}\n"),
    ]);
    let mut config = DartLintConfig::new([DartLintRuleId::NamingConvention]);
    config.naming.ignored_path_prefixes = vec!["lib/gen".to_string()];

    let by_string = lint_project(&project, &config);
    assert!(by_string.diagnostics.is_empty(), "lib/gen is a string prefix of lib/generic");

    config.path_match = DartLintPathMatch::Segment;
    let by_segment = lint_project(&project, &config);
    assert!(!by_segment.diagnostics.is_empty());
    assert!(paths(&by_segment).iter().all(|path| *path == "lib/generic/b.dart"));

    config.naming.ignored_path_prefixes.clear();
    config.exclude.path_prefixes = vec!["lib/gen".to_string()];
    let excluded = lint_project(&project, &config);
    assert!(paths(&excluded).iter().all(|path| *path == "lib/generic/b.dart"));
}

#[test]
fn segment_prefix_import_patterns_stop_at_the_package_name() {
    let project = project(&[(
        "lib/main.dart",
        "import 'package:flutter/material.dart';\nimport 'package:flutter_bloc/flutter_bloc.dart';\nimport 'dart:io';\n",
    )]);
    let mut config = DartLintConfig::new([DartLintRuleId::ForbiddenImport]);
    config.forbidden_imports.push(DartForbiddenImportPattern {
        uri: "package:flutter".to_string(),
        match_kind: DartImportPatternKind::Prefix,
        source_prefix: None,
    });
    assert_eq!(lint_project(&project, &config).diagnostics.len(), 2);

    config.forbidden_imports[0].match_kind = DartImportPatternKind::SegmentPrefix;
    let analysis = lint_project(&project, &config);
    assert_eq!(analysis.diagnostics.len(), 1);
    assert!(analysis.diagnostics[0].message.contains("package:flutter/material.dart"));

    config.forbidden_imports[0].uri = "dart:io".to_string();
    assert_eq!(lint_project(&project, &config).diagnostics.len(), 1);
}

#[test]
fn path_prefix_matching_rules() {
    assert!(DartLintPathMatch::String.has_prefix("lib/ui_kit/a.dart", "lib/ui"));
    assert!(DartLintPathMatch::Segment.has_prefix("lib/ui/a.dart", "lib/ui"));
    assert!(DartLintPathMatch::Segment.has_prefix("lib/ui/a.dart", "lib/ui/"));
    assert!(DartLintPathMatch::Segment.has_prefix("lib/ui", "lib/ui/"));
    assert!(!DartLintPathMatch::Segment.has_prefix("lib/ui_kit/a.dart", "lib/ui"));
    assert!(!DartLintPathMatch::Segment.has_prefix("lib/ui/a.dart", ""));
    assert!(!DartLintPathMatch::Segment.has_prefix("lib/ui/a.dart", "/"));
}
