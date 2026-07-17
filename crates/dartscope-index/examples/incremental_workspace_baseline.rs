use dartscope_core::{DartDiagnostic, DartFileAnalysis, DartProjectAnalysis, DartProjectSummary};
use dartscope_index::DartWorkspaceIndex;

fn main() {
    for file_count in [1_000, 10_000] {
        let files = (0..file_count)
            .map(|index| DartFileAnalysis::empty(format!("lib/file_{index:05}.dart")))
            .collect();
        let project = DartProjectAnalysis {
            root: ".".to_string(),
            files,
            pubspecs: Vec::new(),
            package_configs: Vec::new(),
            summary: DartProjectSummary::default(),
            diagnostics: Vec::new(),
        };
        let mut workspace = DartWorkspaceIndex::from_project(project);
        let mut changed = workspace
            .snapshot()
            .project()
            .files
            .last()
            .expect("synthetic workspace contains files")
            .clone();
        changed.diagnostics.push(
            DartDiagnostic::warning("synthetic", "operation-count baseline", None)
                .with_path(changed.path.clone()),
        );
        let update = workspace.upsert_file(changed);

        let counters = workspace.counters();
        assert_eq!(counters.uri_files_rebuilt, file_count as u64);
        assert_eq!(counters.reference_files_rebuilt, 0);
        println!(
            "files={file_count} affected={} rebuilt={:?} counters={:?}",
            update.affected_paths.len(),
            update.rebuilt,
            counters
        );
    }
}
