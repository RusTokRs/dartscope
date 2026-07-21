use dartscope_core::{
    DartCompilationEnvironment, DartFileInput, DartFileReferenceAnalysis, DartProjectInput,
    DartProjectReferenceAnalysis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartIndexOptions, DartWorkspaceIndex, DartWorkspaceResolutionContext,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

const A: &str = r#"
void helper() {}
"#;

const B: &str = r#"
import 'a.dart';

void consume(Object? input) {}

void run(int seed) {
  var local = seed;
  helper();
  local++;
  consume(local);
}
"#;

const UPDATED_B: &str = r#"
import 'a.dart';

void consume(Object? input) {}

void run(int seed) {
  var renamed = seed;
  helper();
  renamed++;
  consume(renamed);
}
"#;

const BASE: &str = r#"
void conditionalValue() {}
"#;

const IO: &str = r#"
void conditionalValue() {}
"#;

const CONDITIONAL: &str = r#"
import 'base.dart' if (dart.library.io) 'io.dart';
void use() { conditionalValue(); }
"#;

#[test]
fn incremental_snapshots_match_full_navigation_across_updates() {
    let initial = project(B, true);
    let initial_queries = [
        DartDefinitionQuery::new("lib/b.dart", occurrence(B, "helper();", "helper")),
        DartDefinitionQuery::new("lib/b.dart", occurrence(B, "local++;", "local")),
        DartDefinitionQuery::new(
            "lib/conditional.dart",
            occurrence(CONDITIONAL, "conditionalValue();", "conditionalValue"),
        ),
    ];
    let initial_full =
        DartWorkspaceResolutionContext::new(&initial).find_definitions(&initial_queries);

    let mut index = DartWorkspaceIndex::from_reference_project(initial.clone());
    let initial_snapshot = index.snapshot();
    assert_eq!(
        DartWorkspaceResolutionContext::from_snapshot(&initial_snapshot)
            .find_definitions(&initial_queries),
        initial_full
    );
    assert_eq!(initial_snapshot.identifier_references(), initial.references);
    assert_eq!(initial_snapshot.lexical_bindings(), initial.bindings);

    let counters_before_no_op = index.counters();
    let no_op = index.upsert_file_with_references(file_analysis(&initial, "lib/b.dart"));
    assert!(no_op.is_no_op());
    assert_eq!(index.snapshot().generation(), initial_snapshot.generation());
    assert_eq!(
        index.counters().reference_rebuilds,
        counters_before_no_op.reference_rebuilds
    );

    let local_update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/b.dart", UPDATED_B),
    ));
    assert!(local_update.rebuilt.identifier_references);
    let updated_snapshot = index.snapshot();
    let updated_full = project(UPDATED_B, true);
    let updated_queries = [
        DartDefinitionQuery::new("lib/b.dart", occurrence(UPDATED_B, "helper();", "helper")),
        DartDefinitionQuery::new("lib/b.dart", occurrence(UPDATED_B, "renamed++;", "renamed")),
        DartDefinitionQuery::new(
            "lib/conditional.dart",
            occurrence(CONDITIONAL, "conditionalValue();", "conditionalValue"),
        ),
    ];
    assert_eq!(
        DartWorkspaceResolutionContext::from_snapshot(&updated_snapshot)
            .find_definitions(&updated_queries),
        DartWorkspaceResolutionContext::new(&updated_full).find_definitions(&updated_queries)
    );
    assert_eq!(
        DartWorkspaceResolutionContext::from_snapshot(&initial_snapshot)
            .find_definitions(&initial_queries),
        initial_full
    );

    let removal = index.remove_file("lib/a.dart");
    assert!(removal.rebuilt.identifier_references);
    let removed_snapshot = index.snapshot();
    let removed_full = project(UPDATED_B, false);
    let removed_batch = DartWorkspaceResolutionContext::from_snapshot(&removed_snapshot)
        .find_definitions(&updated_queries);
    assert_eq!(
        removed_batch,
        DartWorkspaceResolutionContext::new(&removed_full).find_definitions(&updated_queries)
    );
    assert_eq!(
        resolution_at(
            &removed_batch.resolutions,
            "lib/b.dart",
            occurrence(UPDATED_B, "helper();", "helper"),
        )
        .status,
        DartDefinitionResolutionStatus::Missing
    );

    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );
    let options_update = index.update_options(options.clone());
    assert!(options_update.rebuilt.identifier_references);
    let options_snapshot = index.snapshot();
    let options_batch = DartWorkspaceResolutionContext::from_snapshot(&options_snapshot)
        .find_definitions(&updated_queries);
    assert_eq!(
        options_batch,
        DartWorkspaceResolutionContext::with_options(&removed_full, &options)
            .find_definitions(&updated_queries)
    );
    assert_eq!(
        resolution_at(
            &options_batch.resolutions,
            "lib/conditional.dart",
            occurrence(CONDITIONAL, "conditionalValue();", "conditionalValue"),
        )
        .status,
        DartDefinitionResolutionStatus::Resolved
    );
}

fn project(b_source: &str, include_a: bool) -> DartProjectReferenceAnalysis {
    let mut files = vec![
        DartFileInput::new("lib/b.dart", b_source),
        DartFileInput::new("lib/base.dart", BASE),
        DartFileInput::new("lib/io.dart", IO),
        DartFileInput::new("lib/conditional.dart", CONDITIONAL),
    ];
    if include_a {
        files.push(DartFileInput::new("lib/a.dart", A));
    }
    analyze_project_with_references(DartProjectInput::new(".", files, vec![]))
}

fn file_analysis(analysis: &DartProjectReferenceAnalysis, path: &str) -> DartFileReferenceAnalysis {
    DartFileReferenceAnalysis {
        file: analysis
            .project
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("file analysis")
            .clone(),
        references: analysis
            .references
            .iter()
            .filter(|reference| reference.source_path == path)
            .cloned()
            .collect(),
        bindings: analysis
            .bindings
            .iter()
            .filter(|binding| binding.source_path == path)
            .cloned()
            .collect(),
    }
}

fn resolution_at<'a>(
    resolutions: &'a [DartDefinitionResolution],
    path: &str,
    byte_offset: usize,
) -> &'a DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| {
            resolution.query.source_path == path && resolution.query.byte_offset == byte_offset
        })
        .unwrap_or_else(|| panic!("missing definition result for {path}@{byte_offset}"))
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
