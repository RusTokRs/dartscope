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
