#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "crates/dartscope-index/src/incremental.rs"
INDEX_LIB = ROOT / "crates/dartscope-index/src/lib.rs"
LINT = ROOT / "crates/dartscope-lints/src/incremental.rs"
LINT_LIB = ROOT / "crates/dartscope-lints/src/lib.rs"
EXAMPLE = ROOT / "crates/dartscope-lints/examples/incremental_lint_baseline.rs"
UMBRELLA = ROOT / "crates/dartscope/src/lib.rs"
INDEX_DOC = ROOT / "docs/development/incremental-index.md"
LINT_DOC = ROOT / "docs/development/incremental-lints.md"
ROADMAP = ROOT / "docs/development/dartscope-library-plan.md"
CHANGELOG = ROOT / "CHANGELOG.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    INDEX,
    """/// Derived products rebuilt by one workspace mutation.
""",
    """/// Deterministic retained-cache shape for memory baselines.
///
/// `retained_path_uri_bytes` is the exact UTF-8 payload retained by cache keys and path/URI evidence.
/// It is a stable lower-bound payload metric, not an allocator-specific heap measurement.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartWorkspaceIndexRetainedMetrics {
    pub indexed_files: usize,
    pub uri_source_entries: usize,
    pub uri_references: usize,
    pub library_entries: usize,
    pub library_member_paths: usize,
    pub dependency_fingerprints: usize,
    pub dependency_references: usize,
    pub graphql_library_entries: usize,
    pub graphql_bindings: usize,
    pub graphql_unresolved_uses: usize,
    pub reference_source_entries: usize,
    pub reference_resolutions: usize,
    pub retained_path_uri_bytes: usize,
}

/// Derived products rebuilt by one workspace mutation.
""",
)
replace_once(
    INDEX,
    """    pub const fn counters(&self) -> DartWorkspaceIndexCounters {
        self.counters
    }

    pub fn options(&self) -> &DartIndexOptions {
""",
    """    pub const fn counters(&self) -> DartWorkspaceIndexCounters {
        self.counters
    }

    pub fn retained_metrics(&self) -> DartWorkspaceIndexRetainedMetrics {
        let uri_references = self
            .uri_references_by_path
            .values()
            .map(|references| references.len())
            .sum();
        let library_member_paths = self
            .library_paths_by_owner
            .values()
            .map(|paths| paths.len())
            .sum();
        let dependency_references = self
            .library_dependency_fingerprints_by_owner
            .values()
            .map(|fingerprint| fingerprint.references.len())
            .sum();
        let graphql_bindings = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.bindings.len())
            .sum();
        let graphql_unresolved_uses = self
            .graphql_contracts_by_library
            .values()
            .map(|analysis| analysis.unresolved_uses.len())
            .sum();
        let reference_resolutions = self
            .reference_resolutions_by_path
            .values()
            .map(|resolutions| resolutions.len())
            .sum();
        let retained_path_uri_bytes = self.root.len()
            + self
                .uri_references_by_path
                .iter()
                .map(|(path, references)| {
                    path.len()
                        + references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .library_paths_by_owner
                .iter()
                .map(|(owner, paths)| {
                    owner.len() + paths.iter().map(String::len).sum::<usize>()
                })
                .sum::<usize>()
            + self
                .library_dependency_fingerprints_by_owner
                .iter()
                .map(|(owner, fingerprint)| {
                    owner.len()
                        + fingerprint.owner_path.len()
                        + fingerprint
                            .member_paths
                            .iter()
                            .map(String::len)
                            .sum::<usize>()
                        + fingerprint
                            .references
                            .iter()
                            .map(uri_reference_path_uri_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .graphql_contracts_by_library
                .iter()
                .map(|(owner, analysis)| owner.len() + graphql_contract_path_bytes(analysis))
                .sum::<usize>()
            + self
                .reference_resolutions_by_path
                .iter()
                .map(|(path, resolutions)| {
                    path.len()
                        + resolutions
                            .iter()
                            .map(reference_resolution_path_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>();

        DartWorkspaceIndexRetainedMetrics {
            indexed_files: self.files.len(),
            uri_source_entries: self.uri_references_by_path.len(),
            uri_references,
            library_entries: self.library_paths_by_owner.len(),
            library_member_paths,
            dependency_fingerprints: self.library_dependency_fingerprints_by_owner.len(),
            dependency_references,
            graphql_library_entries: self.graphql_contracts_by_library.len(),
            graphql_bindings,
            graphql_unresolved_uses,
            reference_source_entries: self.reference_resolutions_by_path.len(),
            reference_resolutions,
            retained_path_uri_bytes,
        }
    }

    pub fn options(&self) -> &DartIndexOptions {
""",
)
replace_once(
    INDEX,
    """fn build_uri_reference_cache(
""",
    """fn uri_reference_path_uri_bytes(reference: &DartUriReference) -> usize {
    reference.source_path.len()
        + reference.uri.len()
        + reference.condition.as_ref().map_or(0, String::len)
        + reference.target_path.as_ref().map_or(0, String::len)
        + reference.target_uri.as_ref().map_or(0, String::len)
        + reference
            .candidate_paths
            .iter()
            .map(String::len)
            .sum::<usize>()
}

fn graphql_contract_path_bytes(analysis: &DartGraphqlContractAnalysis) -> usize {
    analysis
        .bindings
        .iter()
        .map(|binding| binding.operation_path.len() + binding.use_path.len())
        .sum::<usize>()
        + analysis
            .unresolved_uses
            .iter()
            .map(|operation_use| {
                operation_use.use_path.len()
                    + operation_use
                        .candidate_paths
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
            })
            .sum::<usize>()
}

fn reference_resolution_path_bytes(
    resolution: &DartIdentifierReferenceResolution,
) -> usize {
    resolution.reference.source_path.len()
        + resolution
            .candidates
            .iter()
            .map(|candidate| candidate.declaration_path.len())
            .sum::<usize>()
}

fn build_uri_reference_cache(
""",
)
replace_once(
    INDEX_LIB,
    """pub use incremental::{
    DartLibraryDependencyFingerprint, DartWorkspaceIndex, DartWorkspaceIndexCounters,
    DartWorkspaceSnapshot, DartWorkspaceSubsystems, DartWorkspaceUpdate,
};
""",
    """pub use incremental::{
    DartLibraryDependencyFingerprint, DartWorkspaceIndex, DartWorkspaceIndexCounters,
    DartWorkspaceIndexRetainedMetrics, DartWorkspaceSnapshot, DartWorkspaceSubsystems,
    DartWorkspaceUpdate,
};
""",
)

replace_once(
    LINT,
    """/// Work performed while applying one workspace update to the lint cache.
""",
    """/// Deterministic retained-cache shape for memory baselines.
///
/// `retained_diagnostic_text_bytes` counts exact UTF-8 diagnostic payload retained by the cache and is
/// intentionally independent of allocator layout.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartIncrementalLintRetainedMetrics {
    pub cached_libraries: usize,
    pub local_diagnostics: usize,
    pub global_diagnostics: usize,
    pub retained_diagnostic_text_bytes: usize,
}

/// Work performed while applying one workspace update to the lint cache.
""",
)
replace_once(
    LINT,
    """    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Applies one workspace update and reuses diagnostics for unaffected libraries.
""",
    """    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn retained_metrics(&self) -> DartIncrementalLintRetainedMetrics {
        let local_diagnostics = self
            .diagnostics_by_library
            .values()
            .map(|diagnostics| diagnostics.len())
            .sum();
        let retained_diagnostic_text_bytes = self
            .diagnostics_by_library
            .values()
            .flat_map(|diagnostics| diagnostics.iter())
            .chain(self.global_diagnostics.iter())
            .map(diagnostic_text_bytes)
            .sum();
        DartIncrementalLintRetainedMetrics {
            cached_libraries: self.diagnostics_by_library.len(),
            local_diagnostics,
            global_diagnostics: self.global_diagnostics.len(),
            retained_diagnostic_text_bytes,
        }
    }

    /// Applies one workspace update and reuses diagnostics for unaffected libraries.
""",
)
replace_once(
    LINT,
    """fn local_rule_ids(config: &DartLintConfig) -> Vec<DartLintRuleId> {
""",
    """fn diagnostic_text_bytes(diagnostic: &DartLintDiagnostic) -> usize {
    diagnostic.path.len()
        + diagnostic.message.len()
        + diagnostic
            .related_paths
            .iter()
            .map(String::len)
            .sum::<usize>()
}

fn local_rule_ids(config: &DartLintConfig) -> Vec<DartLintRuleId> {
""",
)
replace_once(
    LINT_LIB,
    """pub use incremental::{
    DartIncrementalLintCache, DartIncrementalLintCounters, DartIncrementalLintUpdate,
};
""",
    """pub use incremental::{
    DartIncrementalLintCache, DartIncrementalLintCounters, DartIncrementalLintRetainedMetrics,
    DartIncrementalLintUpdate,
};
""",
)

EXAMPLE.parent.mkdir(parents=True, exist_ok=True)
EXAMPLE.write_text('''use std::hint::black_box;
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
    let lint_update = black_box(lint_cache.update(
        snapshot.as_ref(),
        &workspace_update,
        &config,
    ));
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
''', encoding="utf-8")

replace_once(
    UMBRELLA,
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceSnapshot,
""",
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceIndexRetainedMetrics, DartWorkspaceSnapshot,
""",
)
replace_once(
    INDEX_DOC,
    """These are semantic operation counters rather than elapsed-time assertions, so they are deterministic
across Linux, Windows, and differently loaded runners.
""",
    """These are semantic operation counters rather than elapsed-time assertions, so they are deterministic
across Linux, Windows, and differently loaded runners. `retained_metrics()` complements them with cache
entry counts and exact retained path/URI UTF-8 payload bytes, a stable lower-bound memory proxy that does
not claim allocator-specific heap precision.
""",
)
replace_once(
    INDEX_DOC,
    """cargo run -p dartscope-index --example incremental_workspace_baseline --release
```
""",
    """cargo run -p dartscope-index --example incremental_workspace_baseline --release
cargo run -p dartscope-lints --example incremental_lint_baseline --release
```

The lint baseline prints initial-build and single-library update timings for 1k and 10k files. Timings are
informational; correctness gates assert deterministic cache shapes, counters, and full-result equivalence
rather than host-dependent duration thresholds.
""",
)
replace_once(
    LINT_DOC,
    """`DartIncrementalLintCounters` records full rebuilds, local libraries rebuilt, global rebuilds, and lint
updates that required no rule work. These are deterministic semantic-work counters, not elapsed-time
assertions.
""",
    """`DartIncrementalLintCounters` records full rebuilds, local libraries rebuilt, global rebuilds, and lint
updates that required no rule work. `retained_metrics()` reports cached-library and diagnostic counts plus
exact retained diagnostic UTF-8 payload bytes. These are deterministic semantic-work and lower-bound
payload metrics, not allocator or elapsed-time assertions.

Run the informational 1k/10k update-time baseline with:

```text
cargo run -p dartscope-lints --example incremental_lint_baseline --release
```

The example asserts one-library rebuild counters and full lint equivalence. Printed microseconds are never
used as CI pass/fail thresholds.
""",
)
replace_once(
    ROADMAP,
    """Status: in progress. Priority: P1. Prerequisites: DS-INDEX-004, DS-AUDIT-001.
""",
    """Status: verified. Priority: P1. Prerequisites: DS-INDEX-004, DS-AUDIT-001.
""",
)
replace_once(
    ROADMAP,
    """15. **P1 fixed:** the new public dependency-fingerprint model was initially omitted from the umbrella
    crate's explicit index re-export. `dartscope` now exposes the same named type as `dartscope-index`.

Remaining work:

1. Add memory/update-time baselines for the per-library index and lint-cache implementation.
""",
    """15. **P1 fixed:** the new public dependency-fingerprint model was initially omitted from the umbrella
    crate's explicit index re-export. `dartscope` now exposes the same named type as `dartscope-index`.
16. Added deterministic retained-cache metrics for index and lint contexts plus an informational 1k/10k
    initial-build and single-library update-time baseline. CI gates cache shapes, one-library counters, and
    full semantic equivalence while intentionally avoiding host-dependent duration thresholds.

Verification completed (2026-07-18):

- exact Rust 1.95 formatting, focused index/lint fixtures, Clippy, rustdoc, workspace tests, umbrella
  all-features, release package validation, and hosted Linux/Windows checks passed on the verified final
  feature SHA;
- the aggregate `dartscope/ci` status was published as `success` after the final baseline gate.
""",
)
replace_once(
    CHANGELOG,
    """- Snapshot-backed and incremental lint contexts that retain unaffected per-library diagnostics while
  preserving full stateless lint equivalence.
""",
    """- Snapshot-backed and incremental lint contexts that retain unaffected per-library diagnostics while
  preserving full stateless lint equivalence.
- Deterministic retained-cache payload metrics and informational 1k/10k index/lint update-time baselines
  without flaky absolute timing thresholds.
""",
)

print("DS-INDEX-005 cache baseline slice applied")
