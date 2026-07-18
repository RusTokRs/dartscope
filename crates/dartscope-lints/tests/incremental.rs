use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::DartWorkspaceIndex;
use dartscope_lints::{
    DartIncrementalLintCache, DartLintConfig, DartLintRuleId, lint_project, lint_workspace_snapshot,
};
use dartscope_parse::{analyze_file, analyze_project};

#[test]
fn snapshot_lint_matches_stateless_lint_semantics() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/main.dart",
                "import 'bad_name.dart';
part 'missing.dart';
void main() {}
",
            ),
            DartFileInput::new(
                "lib/bad_name.dart",
                "class bad_name {}
",
            ),
            DartFileInput::new(
                "lib/orphan.dart",
                "class Orphan {}
",
            ),
        ],
        vec![],
    ));
    let index = DartWorkspaceIndex::from_project(project);
    let snapshot = index.snapshot();
    let mut config = DartLintConfig::all_rules();
    config.orphan_files.entry_points = vec!["lib/main.dart".to_string()];

    assert_eq!(
        lint_workspace_snapshot(snapshot.as_ref(), &config),
        lint_project(snapshot.project(), &config)
    );
}

#[test]
fn local_lint_diagnostics_rebuild_only_the_affected_library() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/a.dart",
                "class bad_name {}
",
            ),
            DartFileInput::new(
                "lib/b.dart",
                "class also_bad {}
",
            ),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let config = DartLintConfig::new([DartLintRuleId::NamingConvention]);
    let initial_snapshot = index.snapshot();
    let mut cache = DartIncrementalLintCache::new(initial_snapshot.as_ref(), config.clone());
    let before = cache.counters();
    assert_eq!(cache.analysis().diagnostics.len(), 2);

    let workspace_update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/a.dart",
        "class GoodName {}
",
    )));
    let snapshot = index.snapshot();
    let lint_update = cache.update(snapshot.as_ref(), &workspace_update, &config);

    assert!(!lint_update.full_rebuild);
    assert_eq!(lint_update.affected_libraries, vec!["lib/a.dart"]);
    assert_eq!(lint_update.local_libraries_rebuilt, 1);
    assert!(!lint_update.global_rules_rebuilt);
    assert_eq!(
        cache.counters().local_libraries_rebuilt,
        before.local_libraries_rebuilt + 1
    );
    assert_eq!(cache.analysis().diagnostics.len(), 1);
    assert_eq!(cache.analysis().diagnostics[0].path, "lib/b.dart");
    assert_eq!(
        cache.analysis(),
        &lint_workspace_snapshot(snapshot.as_ref(), &config)
    );
}

#[test]
fn uri_changes_rebuild_the_global_orphan_rule() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/main.dart",
                "import 'a.dart';
void main() {}
",
            ),
            DartFileInput::new(
                "lib/a.dart",
                "class A {}
",
            ),
            DartFileInput::new(
                "lib/spare.dart",
                "class Spare {}
",
            ),
        ],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let mut config = DartLintConfig::new([DartLintRuleId::OrphanFile]);
    config.orphan_files.entry_points = vec!["lib/main.dart".to_string()];
    let initial_snapshot = index.snapshot();
    let mut cache = DartIncrementalLintCache::new(initial_snapshot.as_ref(), config.clone());
    let before = cache.counters();
    assert_eq!(cache.analysis().diagnostics.len(), 1);
    assert_eq!(cache.analysis().diagnostics[0].path, "lib/spare.dart");

    let workspace_update = index.upsert_file(analyze_file(DartFileInput::new(
        "lib/main.dart",
        "import 'a.dart';
import 'spare.dart';
void main() {}
",
    )));
    let snapshot = index.snapshot();
    let lint_update = cache.update(snapshot.as_ref(), &workspace_update, &config);

    assert_eq!(lint_update.local_libraries_rebuilt, 0);
    assert!(lint_update.global_rules_rebuilt);
    assert_eq!(cache.counters().global_rebuilds, before.global_rebuilds + 1);
    assert!(cache.analysis().diagnostics.is_empty());
    assert_eq!(
        cache.analysis(),
        &lint_workspace_snapshot(snapshot.as_ref(), &config)
    );
}

#[test]
fn lint_configuration_changes_force_a_safe_full_rebuild() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![DartFileInput::new(
            "lib/a.dart",
            "class bad_name {}
",
        )],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let initial_snapshot = index.snapshot();
    let mut cache =
        DartIncrementalLintCache::new(initial_snapshot.as_ref(), DartLintConfig::default());
    assert!(cache.analysis().diagnostics.is_empty());
    let before = cache.counters();

    let workspace_update = index.upsert_file(initial_snapshot.project().files[0].clone());
    let snapshot = index.snapshot();
    let config = DartLintConfig::new([DartLintRuleId::NamingConvention]);
    let lint_update = cache.update(snapshot.as_ref(), &workspace_update, &config);

    assert!(workspace_update.is_no_op());
    assert!(lint_update.full_rebuild);
    assert_eq!(lint_update.local_libraries_rebuilt, 1);
    assert_eq!(cache.counters().full_rebuilds, before.full_rebuilds + 1);
    assert_eq!(cache.analysis().diagnostics.len(), 1);
    assert_eq!(
        cache.analysis(),
        &lint_workspace_snapshot(snapshot.as_ref(), &config)
    );
}
