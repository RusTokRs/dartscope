use std::hint::black_box;
use std::time::Instant;

use dartscope_core::{
    DartFileAnalysis, DartImport, DartProjectAnalysis, DartProjectSummary, SourceSpan,
};
use dartscope_index::DartWorkspaceIndex;
use dartscope_lints::{
    DartForbiddenImportPattern, DartImportPatternKind, DartIncrementalLintCache, DartLintConfig,
    DartLintRuleId, lint_workspace_snapshot,
};

fn main() {
    for file_count in [1_000_usize, 10_000] {
        run_baseline(file_count);
    }
}

fn run_baseline(file_count: usize) {
    let project = synthetic_project(file_count);

    let started = Instant::now();
    let mut index = black_box(DartWorkspaceIndex::from_project(project));
    let index_build = started.elapsed();

    let mut config = DartLintConfig::new([DartLintRuleId::ForbiddenImport]);
    config.forbidden_imports.push(DartForbiddenImportPattern {
        uri: "file_".to_string(),
        match_kind: DartImportPatternKind::Prefix,
        source_prefix: None,
    });
    let snapshot = index.snapshot();
    let started = Instant::now();
    let mut lint_cache = black_box(DartIncrementalLintCache::new(
        snapshot.as_ref(),
        config.clone(),
    ));
    let lint_build = started.elapsed();

    let index_before = index.counters();
    let lint_before = lint_cache.counters();
    let last_path = file_path(file_count - 1);
    let mut changed = DartFileAnalysis::empty(last_path.clone());
    changed.imports.push(import("file_00000.dart"));

    let started = Instant::now();
    let workspace_update = black_box(index.upsert_file(changed));
    let index_update = started.elapsed();
    let snapshot = index.snapshot();

    let started = Instant::now();
    let lint_update = black_box(lint_cache.update(snapshot.as_ref(), &workspace_update, &config));
    let lint_update_time = started.elapsed();

    assert_eq!(workspace_update.affected_libraries, vec![last_path]);
    assert_eq!(
        index.counters().library_dependency_fingerprints_rebuilt,
        index_before.library_dependency_fingerprints_rebuilt + 1
    );
    assert_eq!(lint_update.local_libraries_rebuilt, 1);
    assert_eq!(
        lint_cache.counters().local_libraries_rebuilt,
        lint_before.local_libraries_rebuilt + 1
    );
    assert_eq!(
        lint_cache.analysis(),
        &lint_workspace_snapshot(snapshot.as_ref(), &config)
    );

    let index_metrics = index.retained_metrics();
    let lint_metrics = lint_cache.retained_metrics();
    assert_eq!(index_metrics.indexed_files, file_count);
    assert_eq!(index_metrics.library_entries, file_count);
    assert_eq!(index_metrics.dependency_fingerprints, file_count);
    assert_eq!(lint_metrics.cached_libraries, file_count);
    assert_eq!(lint_metrics.local_diagnostics, file_count - 1);

    println!(
        "files={file_count} index_build_us={} lint_build_us={} index_update_us={} lint_update_us={} index_metrics={index_metrics:?} lint_metrics={lint_metrics:?}",
        index_build.as_micros(),
        lint_build.as_micros(),
        index_update.as_micros(),
        lint_update_time.as_micros(),
    );
}

fn synthetic_project(file_count: usize) -> DartProjectAnalysis {
    let files = (0..file_count)
        .map(|index| {
            let mut file = DartFileAnalysis::empty(file_path(index));
            if index > 0 {
                file.imports
                    .push(import(&format!("file_{:05}.dart", index - 1)));
            }
            file
        })
        .collect();
    DartProjectAnalysis {
        root: ".".to_string(),
        files,
        pubspecs: Vec::new(),
        package_configs: Vec::new(),
        summary: DartProjectSummary::default(),
        diagnostics: Vec::new(),
    }
}

fn file_path(index: usize) -> String {
    format!("lib/file_{index:05}.dart")
}

fn import(uri: &str) -> DartImport {
    let source = format!("import '{uri}';");
    DartImport {
        uri: uri.to_string(),
        configurations: Vec::new(),
        is_deferred: false,
        prefix: None,
        combinators: Vec::new(),
        span: SourceSpan::line(1, 0, &source),
    }
}
