use dartscope_core::{DartFileInput, DartProjectInput, DiagnosticSeverity};
use dartscope_lints::{
    DartForbiddenImportPattern, DartImportPatternKind, DartLayerBoundary, DartLintConfig,
    DartLintRuleId, DartLintSeverityOverride, DartOrphanFileRuleConfig, lint_project,
};
use dartscope_parse::analyze_project;

#[test]
fn default_configuration_is_explicitly_disabled() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/BadName.dart", "class bad_type {}")],
        vec![],
    ));

    let analysis = lint_project(&project, &DartLintConfig::default());

    assert!(analysis.diagnostics.is_empty());
    assert_eq!(analysis.summary.enabled_rules, 0);
}

#[test]
fn runs_configured_import_layer_naming_and_part_rules_deterministically() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/data/private_api.dart", "class PrivateApi {}"),
            DartFileInput::new(
                "lib/ui/BadScreen.dart",
                "import 'dart:io';
import '../data/private_api.dart';
part 'missing.dart';
class bad_screen {}
void BadFunction() {}
",
            ),
        ],
        vec![],
    ));
    let mut config = DartLintConfig::new([
        DartLintRuleId::UnresolvedPart,
        DartLintRuleId::NamingConvention,
        DartLintRuleId::LayerBoundary,
        DartLintRuleId::ForbiddenImport,
    ]);
    config.forbidden_imports.push(DartForbiddenImportPattern {
        uri: "dart:io".to_string(),
        match_kind: DartImportPatternKind::Exact,
        source_prefix: Some("lib/ui/".to_string()),
    });
    config.layer_boundaries.push(DartLayerBoundary {
        source_prefix: "lib/ui/".to_string(),
        denied_target_prefixes: vec!["lib/data/".to_string()],
    });
    config.severity_overrides.push(DartLintSeverityOverride {
        rule_id: DartLintRuleId::ForbiddenImport,
        severity: DiagnosticSeverity::Error,
    });

    let analysis = lint_project(&project, &config);

    assert_eq!(analysis.summary.enabled_rules, 4);
    assert_eq!(analysis.summary.diagnostics, 6);
    assert_eq!(analysis.summary.errors, 1);
    assert_eq!(analysis.summary.warnings, 5);
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id)
            .collect::<Vec<_>>(),
        [
            DartLintRuleId::NamingConvention,
            DartLintRuleId::ForbiddenImport,
            DartLintRuleId::LayerBoundary,
            DartLintRuleId::UnresolvedPart,
            DartLintRuleId::NamingConvention,
            DartLintRuleId::NamingConvention,
        ]
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.related_paths == ["lib/data/private_api.dart"])
    );
}

#[test]
fn orphan_files_use_explicit_graph_roots() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/main.dart",
                "import 'src/reachable.dart';
",
            ),
            DartFileInput::new(
                "lib/src/reachable.dart",
                "class Reachable {}
",
            ),
            DartFileInput::new(
                "lib/src/orphan.dart",
                "class Orphan {}
",
            ),
            DartFileInput::new(
                "test/helper.dart",
                "class Helper {}
",
            ),
        ],
        vec![],
    ));
    let mut config = DartLintConfig::new([DartLintRuleId::OrphanFile]);
    config.orphan_files = DartOrphanFileRuleConfig {
        entry_points: vec!["lib/main.dart".to_string()],
        ignored_path_prefixes: vec!["test/".to_string()],
    };

    let analysis = lint_project(&project, &config);

    assert_eq!(analysis.diagnostics.len(), 1);
    assert_eq!(analysis.diagnostics[0].rule_id, DartLintRuleId::OrphanFile);
    assert_eq!(analysis.diagnostics[0].path, "lib/src/orphan.dart");
    assert_eq!(
        analysis.diagnostics[0].related_paths,
        ["lib/main.dart".to_string()]
    );
}

#[test]
fn rule_ids_have_stable_serialized_names() {
    assert_eq!(
        serde_json::to_string(&DartLintRuleId::LayerBoundary).unwrap(),
        "\"dartscope.layer_boundary\""
    );
}

#[test]
fn naming_convention_accepts_dollar_names_and_unnamed_extensions() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/generated.dart",
            r#"
class Widget$Base {
  int count$ = 0;
}

void _$register() {}

final version$ = 1;

extension on List<int> {
  int get total$ => length;
}
"#,
        )],
        vec![],
    ));
    let config = DartLintConfig::new([DartLintRuleId::NamingConvention]);

    let analysis = lint_project(&project, &config);

    assert!(
        analysis.diagnostics.is_empty(),
        "unexpected naming findings: {:?}",
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.path.as_str(), diagnostic.message.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn naming_convention_still_reports_a_snake_case_declaration() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/bad_screen.dart", "class bad_screen {}\n"),
            DartFileInput::new("lib/generated.dart", "class Widget$Base {}\n"),
        ],
        vec![],
    ));
    let config = DartLintConfig::new([DartLintRuleId::NamingConvention]);

    let analysis = lint_project(&project, &config);

    assert_eq!(analysis.diagnostics.len(), 1);
    assert_eq!(analysis.diagnostics[0].path, "lib/bad_screen.dart");
    assert_eq!(
        analysis.diagnostics[0].rule_id,
        DartLintRuleId::NamingConvention
    );
}

#[test]
fn naming_convention_skips_generated_names_that_contain_a_dollar_sign() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/user.g.dart",
            "Object _$UserFromJson(Object json) => json;\nvoid jni$_init() {}\nclass $Foo_Bar {}\n",
        )],
        vec![],
    ));
    let config = DartLintConfig::new([DartLintRuleId::NamingConvention]);

    let analysis = lint_project(&project, &config);

    assert!(
        analysis.diagnostics.is_empty(),
        "generated names are not the author's: {:?}",
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn forbidden_import_also_checks_exports_and_conditional_alternatives() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/api.dart",
            "import 'package:ok/ok.dart' if (dart.library.io) 'package:forbidden/io.dart';\n\
             export 'package:forbidden/exported.dart';\n\
             export 'package:ok/ok.dart' if (dart.library.html) 'package:forbidden/html.dart';\n\
             import 'package:forbidden/imported.dart';\n\
             import 'package:forbidden_but_not_really/x.dart';\n",
        )],
        vec![],
    ));
    let mut config = DartLintConfig::new([DartLintRuleId::ForbiddenImport]);
    config.forbidden_imports.push(DartForbiddenImportPattern {
        uri: "package:forbidden/".to_string(),
        match_kind: DartImportPatternKind::Prefix,
        source_prefix: None,
    });

    let analysis = lint_project(&project, &config);

    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        [
            "import `package:forbidden/io.dart` is forbidden by pattern `package:forbidden/`",
            "export `package:forbidden/exported.dart` is forbidden by pattern `package:forbidden/`",
            "export `package:forbidden/html.dart` is forbidden by pattern `package:forbidden/`",
            "import `package:forbidden/imported.dart` is forbidden by pattern `package:forbidden/`",
        ]
    );
}

#[test]
fn layer_boundary_applies_to_exports() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/data/repo.dart", "class Repo {}\n"),
            DartFileInput::new("lib/ui/screen.dart", "export '../data/repo.dart';\n"),
            DartFileInput::new("lib/ui/part_of_ui.dart", "part '../data/repo.dart';\n"),
        ],
        vec![],
    ));
    let mut config = DartLintConfig::new([DartLintRuleId::LayerBoundary]);
    config.layer_boundaries.push(DartLayerBoundary {
        source_prefix: "lib/ui/".to_string(),
        denied_target_prefixes: vec!["lib/data/".to_string()],
    });

    let analysis = lint_project(&project, &config);

    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    assert_eq!(analysis.diagnostics[0].path, "lib/ui/screen.dart");
    assert!(
        analysis.diagnostics[0]
            .message
            .contains("must not export target `lib/data/repo.dart`"),
        "{}",
        analysis.diagnostics[0].message
    );
}

#[test]
fn orphan_files_report_an_entry_point_that_is_not_an_analyzed_file() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/main.dart", "import 'src/used.dart';\n"),
            DartFileInput::new("lib/src/used.dart", "class Used {}\n"),
            DartFileInput::new("lib/src/orphan.dart", "class Orphan {}\n"),
        ],
        vec![],
    ));
    let mut config = DartLintConfig::new([DartLintRuleId::OrphanFile]);
    config.orphan_files = DartOrphanFileRuleConfig {
        entry_points: vec!["lib/main.dart".to_string(), "lib/mian.dart".to_string()],
        ignored_path_prefixes: vec![],
    };

    let analysis = lint_project(&project, &config);

    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.path.as_str())
            .collect::<Vec<_>>(),
        ["lib/mian.dart", "lib/src/orphan.dart"]
    );
    assert!(analysis.diagnostics[0].message.contains("entry point"));

    config.orphan_files.entry_points = vec!["lib/mian.dart".to_string()];
    let only_missing = lint_project(&project, &config);
    assert_eq!(only_missing.diagnostics.len(), 1);
    assert_eq!(only_missing.diagnostics[0].path, "lib/mian.dart");
}
