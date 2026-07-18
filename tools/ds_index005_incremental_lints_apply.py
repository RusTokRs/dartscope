#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTEXT = ROOT / "crates/dartscope-lints/src/context.rs"
ENGINE = ROOT / "crates/dartscope-lints/src/engine.rs"
INCREMENTAL = ROOT / "crates/dartscope-lints/src/incremental.rs"
LIB = ROOT / "crates/dartscope-lints/src/lib.rs"
FORBIDDEN = ROOT / "crates/dartscope-lints/src/rules/forbidden_imports.rs"
LAYER = ROOT / "crates/dartscope-lints/src/rules/layer_boundaries.rs"
NAMING = ROOT / "crates/dartscope-lints/src/rules/naming.rs"
UNRESOLVED = ROOT / "crates/dartscope-lints/src/rules/unresolved_parts.rs"
ORPHAN = ROOT / "crates/dartscope-lints/src/rules/orphan_files.rs"
TESTS = ROOT / "crates/dartscope-lints/tests/incremental.rs"
DOC = ROOT / "docs/development/incremental-lints.md"
LINT_DOC = ROOT / "docs/development/lint-rules.md"
INDEX_DOC = ROOT / "docs/development/incremental-index.md"
ROADMAP = ROOT / "docs/development/dartscope-library-plan.md"
CHANGELOG = ROOT / "CHANGELOG.md"
UMBRELLA = ROOT / "crates/dartscope/src/lib.rs"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


CONTEXT.write_text('''use std::borrow::Cow;
use std::collections::BTreeSet;

use dartscope_core::{DartPartLinkAnalysis, DartProjectAnalysis, DartUriGraph};
use dartscope_index::{
    DartWorkspaceSnapshot, analyze_part_links, build_uri_graph,
};

use crate::DartLintRuleId;

pub(crate) struct RuleContext<'a> {
    pub(crate) project: &'a DartProjectAnalysis,
    uri_graph: Option<Cow<'a, DartUriGraph>>,
    part_links: Option<Cow<'a, DartPartLinkAnalysis>>,
    included_paths: Option<&'a BTreeSet<String>>,
}

impl<'a> RuleContext<'a> {
    pub(crate) fn new(project: &'a DartProjectAnalysis, enabled: &[DartLintRuleId]) -> Self {
        let (needs_uri_graph, needs_part_links) = requirements(enabled);
        Self {
            project,
            uri_graph: needs_uri_graph.then(|| Cow::Owned(build_uri_graph(project))),
            part_links: needs_part_links.then(|| Cow::Owned(analyze_part_links(project))),
            included_paths: None,
        }
    }

    pub(crate) fn from_snapshot(
        snapshot: &'a DartWorkspaceSnapshot,
        enabled: &[DartLintRuleId],
        included_paths: Option<&'a BTreeSet<String>>,
    ) -> Self {
        let (needs_uri_graph, needs_part_links) = requirements(enabled);
        Self {
            project: snapshot.project(),
            uri_graph: needs_uri_graph.then(|| Cow::Borrowed(snapshot.uri_graph())),
            part_links: needs_part_links.then(|| Cow::Borrowed(snapshot.part_links())),
            included_paths,
        }
    }

    pub(crate) fn includes_path(&self, path: &str) -> bool {
        self.included_paths
            .map(|included| included.contains(path))
            .unwrap_or(true)
    }

    pub(crate) fn uri_graph(&self) -> Option<&DartUriGraph> {
        self.uri_graph.as_deref()
    }

    pub(crate) fn part_links(&self) -> Option<&DartPartLinkAnalysis> {
        self.part_links.as_deref()
    }
}

fn requirements(enabled: &[DartLintRuleId]) -> (bool, bool) {
    let needs_uri_graph = enabled.iter().any(|rule| {
        matches!(
            rule,
            DartLintRuleId::LayerBoundary | DartLintRuleId::OrphanFile
        )
    });
    let needs_part_links = enabled.contains(&DartLintRuleId::UnresolvedPart);
    (needs_uri_graph, needs_part_links)
}
''', encoding="utf-8")

ENGINE.write_text('''use std::collections::BTreeSet;

use dartscope_core::DiagnosticSeverity;
use dartscope_index::DartWorkspaceSnapshot;

use crate::context::RuleContext;
use crate::model::{DartLintAnalysis, DartLintDiagnostic, DartLintSummary};
use crate::rules;
use crate::{DartLintConfig, DartLintRuleId};

/// Runs enabled rules over normalized project analysis.
pub fn lint_project(
    project: &dartscope_core::DartProjectAnalysis,
    config: &DartLintConfig,
) -> DartLintAnalysis {
    let enabled = config.enabled_rule_ids();
    let context = RuleContext::new(project, &enabled);
    lint_with_context(&context, config, &enabled)
}

/// Runs enabled rules over one immutable workspace snapshot without rebuilding index products.
pub fn lint_workspace_snapshot(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
) -> DartLintAnalysis {
    let enabled = config.enabled_rule_ids();
    lint_workspace_rules(snapshot, config, &enabled)
}

pub(crate) fn lint_workspace_rules(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
) -> DartLintAnalysis {
    let context = RuleContext::from_snapshot(snapshot, enabled, None);
    lint_with_context(&context, config, enabled)
}

pub(crate) fn lint_workspace_paths(
    snapshot: &DartWorkspaceSnapshot,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
    included_paths: &BTreeSet<String>,
) -> DartLintAnalysis {
    let context = RuleContext::from_snapshot(snapshot, enabled, Some(included_paths));
    lint_with_context(&context, config, enabled)
}

fn lint_with_context(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    enabled: &[DartLintRuleId],
) -> DartLintAnalysis {
    let mut diagnostics = Vec::new();

    for rule_id in enabled {
        match rule_id {
            DartLintRuleId::ForbiddenImport => {
                rules::forbidden_imports::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::LayerBoundary => {
                rules::layer_boundaries::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::NamingConvention => {
                rules::naming::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::UnresolvedPart => {
                rules::unresolved_parts::run(context, config, &mut diagnostics)
            }
            DartLintRuleId::OrphanFile => {
                rules::orphan_files::run(context, config, &mut diagnostics)
            }
        }
    }

    analysis_from_diagnostics(enabled.len(), diagnostics)
}

pub(crate) fn analysis_from_diagnostics(
    enabled_rules: usize,
    mut diagnostics: Vec<DartLintDiagnostic>,
) -> DartLintAnalysis {
    sort_and_deduplicate(&mut diagnostics);
    let summary = summarize(enabled_rules, &diagnostics);
    DartLintAnalysis {
        diagnostics,
        summary,
    }
}

fn sort_and_deduplicate(diagnostics: &mut Vec<DartLintDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        (
            &left.path,
            left.span.as_ref().map(|span| span.byte_start),
            left.span.as_ref().map(|span| span.byte_end),
            left.rule_id,
            &left.message,
            &left.related_paths,
        )
            .cmp(&(
                &right.path,
                right.span.as_ref().map(|span| span.byte_start),
                right.span.as_ref().map(|span| span.byte_end),
                right.rule_id,
                &right.message,
                &right.related_paths,
            ))
    });
    diagnostics.dedup();
}

fn summarize(enabled_rules: usize, diagnostics: &[DartLintDiagnostic]) -> DartLintSummary {
    let mut summary = DartLintSummary {
        enabled_rules,
        diagnostics: diagnostics.len(),
        ..DartLintSummary::default()
    };
    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Info => summary.info += 1,
            DiagnosticSeverity::Warning => summary.warnings += 1,
            DiagnosticSeverity::Error => summary.errors += 1,
        }
    }
    summary
}
''', encoding="utf-8")

INCREMENTAL.write_text('''use std::collections::{BTreeMap, BTreeSet};

use dartscope_index::{DartWorkspaceSnapshot, DartWorkspaceUpdate};

use crate::engine::{
    analysis_from_diagnostics, lint_workspace_paths, lint_workspace_rules, lint_workspace_snapshot,
};
use crate::{DartLintAnalysis, DartLintConfig, DartLintDiagnostic, DartLintRuleId};

/// Deterministic semantic-work counters for incremental lint reuse.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct DartIncrementalLintCounters {
    pub full_rebuilds: u64,
    pub local_libraries_rebuilt: u64,
    pub global_rebuilds: u64,
    pub no_op_updates: u64,
}

/// Work performed while applying one workspace update to the lint cache.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DartIncrementalLintUpdate {
    pub generation: u64,
    pub affected_libraries: Vec<String>,
    pub full_rebuild: bool,
    pub local_libraries_rebuilt: usize,
    pub global_rules_rebuilt: bool,
}

/// Stateful lint diagnostics layered over immutable workspace snapshots.
///
/// The cache owns no parser or filesystem state. Local rules are cached per Dart library using the
/// snapshot's dependency fingerprints. The global orphan-file rule is recomputed only when the URI
/// graph or lint configuration changes.
#[derive(Debug, Clone)]
pub struct DartIncrementalLintCache {
    generation: u64,
    config: DartLintConfig,
    diagnostics_by_library: BTreeMap<String, Vec<DartLintDiagnostic>>,
    global_diagnostics: Vec<DartLintDiagnostic>,
    analysis: DartLintAnalysis,
    counters: DartIncrementalLintCounters,
}

impl DartIncrementalLintCache {
    /// Builds a lint cache from one immutable workspace generation.
    pub fn new(snapshot: &DartWorkspaceSnapshot, config: DartLintConfig) -> Self {
        let analysis = lint_workspace_snapshot(snapshot, &config);
        let (diagnostics_by_library, global_diagnostics) =
            partition_analysis(snapshot, analysis);
        let local_enabled = !local_rule_ids(&config).is_empty();
        let global_enabled = orphan_enabled(&config);
        let mut cache = Self {
            generation: snapshot.generation(),
            config,
            diagnostics_by_library,
            global_diagnostics,
            analysis: DartLintAnalysis::default(),
            counters: DartIncrementalLintCounters {
                full_rebuilds: 1,
                local_libraries_rebuilt: if local_enabled {
                    snapshot.library_dependency_fingerprints().len() as u64
                } else {
                    0
                },
                global_rebuilds: u64::from(global_enabled),
                no_op_updates: 0,
            },
        };
        cache.refresh_analysis();
        cache
    }

    pub fn analysis(&self) -> &DartLintAnalysis {
        &self.analysis
    }

    pub const fn counters(&self) -> DartIncrementalLintCounters {
        self.counters
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Applies one workspace update and reuses diagnostics for unaffected libraries.
    pub fn update(
        &mut self,
        snapshot: &DartWorkspaceSnapshot,
        workspace_update: &DartWorkspaceUpdate,
        config: &DartLintConfig,
    ) -> DartIncrementalLintUpdate {
        let sequence_mismatch = workspace_update.generation != snapshot.generation()
            || workspace_update.generation < self.generation
            || workspace_update.generation > self.generation.saturating_add(1);
        if self.config != *config || sequence_mismatch {
            return self.rebuild_full(snapshot, config.clone());
        }

        self.generation = snapshot.generation();
        if workspace_update.is_no_op() {
            self.counters.no_op_updates += 1;
            return DartIncrementalLintUpdate {
                generation: self.generation,
                affected_libraries: Vec::new(),
                full_rebuild: false,
                local_libraries_rebuilt: 0,
                global_rules_rebuilt: false,
            };
        }

        let local_rules = local_rule_ids(config);
        let active_libraries = active_library_paths(snapshot);
        self.diagnostics_by_library
            .retain(|owner, _| active_libraries.contains_key(owner));

        let mut local_libraries_rebuilt = 0_usize;
        if !local_rules.is_empty() {
            for owner in &workspace_update.affected_libraries {
                self.diagnostics_by_library.remove(owner);
                local_libraries_rebuilt += 1;
                let Some(included_paths) = active_libraries.get(owner) else {
                    continue;
                };
                let analysis = lint_workspace_paths(
                    snapshot,
                    config,
                    &local_rules,
                    included_paths,
                );
                self.diagnostics_by_library
                    .insert(owner.clone(), analysis.diagnostics);
            }
        }

        let global_rules_rebuilt = orphan_enabled(config) && workspace_update.rebuilt.uri_graph;
        if global_rules_rebuilt {
            self.global_diagnostics = lint_workspace_rules(
                snapshot,
                config,
                &[DartLintRuleId::OrphanFile],
            )
            .diagnostics;
        }

        self.counters.local_libraries_rebuilt += local_libraries_rebuilt as u64;
        self.counters.global_rebuilds += u64::from(global_rules_rebuilt);
        if local_libraries_rebuilt == 0 && !global_rules_rebuilt {
            self.counters.no_op_updates += 1;
        }
        self.refresh_analysis();

        DartIncrementalLintUpdate {
            generation: self.generation,
            affected_libraries: workspace_update.affected_libraries.clone(),
            full_rebuild: false,
            local_libraries_rebuilt,
            global_rules_rebuilt,
        }
    }

    fn rebuild_full(
        &mut self,
        snapshot: &DartWorkspaceSnapshot,
        config: DartLintConfig,
    ) -> DartIncrementalLintUpdate {
        let analysis = lint_workspace_snapshot(snapshot, &config);
        let (diagnostics_by_library, global_diagnostics) =
            partition_analysis(snapshot, analysis);
        let affected_libraries: Vec<_> = snapshot
            .library_dependency_fingerprints()
            .iter()
            .map(|fingerprint| fingerprint.owner_path.clone())
            .collect();
        let local_libraries_rebuilt = if local_rule_ids(&config).is_empty() {
            0
        } else {
            affected_libraries.len()
        };
        let global_rules_rebuilt = orphan_enabled(&config);

        self.generation = snapshot.generation();
        self.config = config;
        self.diagnostics_by_library = diagnostics_by_library;
        self.global_diagnostics = global_diagnostics;
        self.counters.full_rebuilds += 1;
        self.counters.local_libraries_rebuilt += local_libraries_rebuilt as u64;
        self.counters.global_rebuilds += u64::from(global_rules_rebuilt);
        self.refresh_analysis();

        DartIncrementalLintUpdate {
            generation: self.generation,
            affected_libraries,
            full_rebuild: true,
            local_libraries_rebuilt,
            global_rules_rebuilt,
        }
    }

    fn refresh_analysis(&mut self) {
        let diagnostics = self
            .diagnostics_by_library
            .values()
            .flat_map(|diagnostics| diagnostics.iter().cloned())
            .chain(self.global_diagnostics.iter().cloned())
            .collect();
        self.analysis = analysis_from_diagnostics(
            self.config.enabled_rule_ids().len(),
            diagnostics,
        );
    }
}

fn local_rule_ids(config: &DartLintConfig) -> Vec<DartLintRuleId> {
    config
        .enabled_rule_ids()
        .into_iter()
        .filter(|rule| *rule != DartLintRuleId::OrphanFile)
        .collect()
}

fn orphan_enabled(config: &DartLintConfig) -> bool {
    config
        .enabled_rule_ids()
        .contains(&DartLintRuleId::OrphanFile)
}

fn active_library_paths(
    snapshot: &DartWorkspaceSnapshot,
) -> BTreeMap<String, BTreeSet<String>> {
    snapshot
        .library_dependency_fingerprints()
        .iter()
        .map(|fingerprint| {
            (
                fingerprint.owner_path.clone(),
                fingerprint.member_paths.iter().cloned().collect(),
            )
        })
        .collect()
}

fn partition_analysis(
    snapshot: &DartWorkspaceSnapshot,
    analysis: DartLintAnalysis,
) -> (
    BTreeMap<String, Vec<DartLintDiagnostic>>,
    Vec<DartLintDiagnostic>,
) {
    let mut diagnostics_by_library: BTreeMap<_, Vec<_>> = snapshot
        .library_dependency_fingerprints()
        .iter()
        .map(|fingerprint| (fingerprint.owner_path.clone(), Vec::new()))
        .collect();
    let owner_by_path: BTreeMap<_, _> = snapshot
        .library_dependency_fingerprints()
        .iter()
        .flat_map(|fingerprint| {
            fingerprint
                .member_paths
                .iter()
                .map(|path| (path.clone(), fingerprint.owner_path.clone()))
        })
        .collect();
    let mut global_diagnostics = Vec::new();

    for diagnostic in analysis.diagnostics {
        if diagnostic.rule_id == DartLintRuleId::OrphanFile {
            global_diagnostics.push(diagnostic);
            continue;
        }
        let Some(owner) = owner_by_path.get(&diagnostic.path) else {
            global_diagnostics.push(diagnostic);
            continue;
        };
        diagnostics_by_library
            .entry(owner.clone())
            .or_default()
            .push(diagnostic);
    }

    (diagnostics_by_library, global_diagnostics)
}
''', encoding="utf-8")

replace_once(
    LIB,
    """mod engine;
mod model;
""",
    """mod engine;
mod incremental;
mod model;
""",
)
replace_once(
    LIB,
    """pub use engine::lint_project;
pub use model::{DartLintAnalysis, DartLintDiagnostic, DartLintSummary};
""",
    """pub use engine::{lint_project, lint_workspace_snapshot};
pub use incremental::{
    DartIncrementalLintCache, DartIncrementalLintCounters, DartIncrementalLintUpdate,
};
pub use model::{DartLintAnalysis, DartLintDiagnostic, DartLintSummary};
""",
)

replace_once(
    FORBIDDEN,
    """    for file in &context.project.files {
        for import in &file.imports {
""",
    """    for file in &context.project.files {
        if !context.includes_path(&file.path) {
            continue;
        }
        for import in &file.imports {
""",
)
replace_once(
    LAYER,
    """    let Some(uri_graph) = &context.uri_graph else {
""",
    """    let Some(uri_graph) = context.uri_graph() else {
""",
)
replace_once(
    LAYER,
    """    for reference in &uri_graph.references {
        if reference.kind != DartUriReferenceKind::Import
""",
    """    for reference in &uri_graph.references {
        if !context.includes_path(&reference.source_path)
            || reference.kind != DartUriReferenceKind::Import
""",
)
replace_once(
    NAMING,
    """    for file in &context.project.files {
        if ignored(&file.path, &config.naming.ignored_path_prefixes) {
""",
    """    for file in &context.project.files {
        if !context.includes_path(&file.path)
            || ignored(&file.path, &config.naming.ignored_path_prefixes)
        {
""",
)
replace_once(
    UNRESOLVED,
    """    let Some(part_links) = &context.part_links else {
""",
    """    let Some(part_links) = context.part_links() else {
""",
)
replace_once(
    UNRESOLVED,
    """    for link in &part_links.links {
        if link.status == DartPartLinkStatus::Matched {
""",
    """    for link in &part_links.links {
        if !context.includes_path(&link.owner_path)
            || link.status == DartPartLinkStatus::Matched
        {
""",
)
replace_once(
    ORPHAN,
    """    let Some(uri_graph) = &context.uri_graph else {
""",
    """    let Some(uri_graph) = context.uri_graph() else {
""",
)

TESTS.parent.mkdir(parents=True, exist_ok=True)
TESTS.write_text('''use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::DartWorkspaceIndex;
use dartscope_lints::{
    DartIncrementalLintCache, DartLintConfig, DartLintRuleId, lint_project,
    lint_workspace_snapshot,
};
use dartscope_parse::{analyze_file, analyze_project};

#[test]
fn snapshot_lint_matches_stateless_lint_semantics() {
    let project = analyze_project(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new(
                "lib/main.dart",
                "import 'bad_name.dart';\npart 'missing.dart';\nvoid main() {}\n",
            ),
            DartFileInput::new("lib/bad_name.dart", "class bad_name {}\n"),
            DartFileInput::new("lib/orphan.dart", "class Orphan {}\n"),
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
            DartFileInput::new("lib/a.dart", "class bad_name {}\n"),
            DartFileInput::new("lib/b.dart", "class also_bad {}\n"),
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
        "class GoodName {}\n",
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
            DartFileInput::new("lib/main.dart", "import 'a.dart';\nvoid main() {}\n"),
            DartFileInput::new("lib/a.dart", "class A {}\n"),
            DartFileInput::new("lib/spare.dart", "class Spare {}\n"),
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
        "import 'a.dart';\nimport 'spare.dart';\nvoid main() {}\n",
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
        vec![DartFileInput::new("lib/a.dart", "class bad_name {}\n")],
        vec![],
    ));
    let mut index = DartWorkspaceIndex::from_project(project);
    let initial_snapshot = index.snapshot();
    let mut cache = DartIncrementalLintCache::new(
        initial_snapshot.as_ref(),
        DartLintConfig::default(),
    );
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
''', encoding="utf-8")

DOC.write_text('''---
id: doc://docs/development/incremental-lints.md
kind: development_contract
language: en
source_language: en
status: active
---

# Incremental Lint Contexts

`dartscope-lints` can consume immutable `DartWorkspaceSnapshot` values without rebuilding URI graphs or
part-link analyses. The dependency direction remains `dartscope-lints -> dartscope-index`; the index crate
contains no lint configuration, rule IDs, or diagnostics.

## APIs

- `lint_workspace_snapshot` is a full semantic equivalent of `lint_project` for callers that already own
  a workspace index.
- `DartIncrementalLintCache::new` partitions a full lint result by normalized Dart library owner.
- `DartIncrementalLintCache::update` consumes `DartWorkspaceUpdate::affected_libraries` and re-runs local
  rules only for those libraries.
- `analysis()` returns the complete deterministic aggregate, including retained findings from unaffected
  libraries.

## Rule Scope

Forbidden-import, layer-boundary, naming, and unresolved-part findings are cached by library. Part files
share their matched owner. The orphan-file rule is global because reachability can cross every library; it
is recomputed when the URI graph or lint configuration changes.

Configuration changes and skipped/out-of-order workspace generations trigger a safe full rebuild rather
than reusing possibly stale findings. The cache stores normalized models and diagnostics only. It performs
no filesystem I/O, source parsing, SDK invocation, or hidden synchronization.

## Counters

`DartIncrementalLintCounters` records full rebuilds, local libraries rebuilt, global rebuilds, and lint
updates that required no rule work. These are deterministic semantic-work counters, not elapsed-time
assertions.
''', encoding="utf-8")

replace_once(
    LINT_DOC,
    """- `lint_project(&DartProjectAnalysis, &DartLintConfig) -> DartLintAnalysis`
- `DartLintRuleId::ALL` lists built-in rules in stable execution order.
""",
    """- `lint_project(&DartProjectAnalysis, &DartLintConfig) -> DartLintAnalysis`
- `lint_workspace_snapshot(&DartWorkspaceSnapshot, &DartLintConfig) -> DartLintAnalysis` reuses the
  snapshot's URI graph and part links.
- `DartIncrementalLintCache` retains local diagnostics by Dart library and consumes
  `DartWorkspaceUpdate::affected_libraries`.
- `DartLintRuleId::ALL` lists built-in rules in stable execution order.
""",
)
replace_once(
    LINT_DOC,
    """The crate remains an optional umbrella feature. `dartscope lint` is a separate filesystem adapter
that maps versioned TOML into this API and emits `dartscope.lint-analysis` v1 or SARIF 2.1.0 without
moving rule semantics into the CLI crate. See `docs/development/lint-cli.md`.
""",
    """The crate remains an optional umbrella feature. `dartscope lint` is a separate filesystem adapter
that maps versioned TOML into this API and emits `dartscope.lint-analysis` v1 or SARIF 2.1.0 without
moving rule semantics into the CLI crate. See `docs/development/lint-cli.md` and
`docs/development/incremental-lints.md`.
""",
)
replace_once(
    INDEX_DOC,
    """publish deterministic fingerprints without exposing mutable cache storage, and updates publish the same
normalized affected-library owners that the next lint-context slice will consume. The public stateless
APIs remain available.
""",
    """publish deterministic fingerprints without exposing mutable cache storage, and updates publish normalized
affected-library owners consumed by `DartIncrementalLintCache`. The dependency remains one-way from the
optional lint crate to the index crate, and the public stateless APIs remain available.
""",
)
replace_once(
    ROADMAP,
    """13. Added retained per-library import/export dependency fingerprints and deterministic affected-library
    owners on every workspace update. Fingerprints preserve exact URI-resolution evidence while unchanged
    library entries remain shared across generations.

Remaining work:

1. Feed the same affected-library evidence into lint contexts without introducing an index/lint
   dependency cycle.
2. Add memory/update-time baselines for the per-library cache implementation.
""",
    """13. Added retained per-library import/export dependency fingerprints and deterministic affected-library
    owners on every workspace update. Fingerprints preserve exact URI-resolution evidence while unchanged
    library entries remain shared across generations.
14. Added snapshot-backed lint execution plus `DartIncrementalLintCache`. Local rules retain diagnostics
    per affected library, the global orphan rule follows URI-graph changes, and configuration or generation
    mismatches fall back to a safe full rebuild without introducing an index/lint dependency cycle.
15. **P1 fixed:** the new public dependency-fingerprint model was initially omitted from the umbrella
    crate's explicit index re-export. `dartscope` now exposes the same named type as `dartscope-index`.

Remaining work:

1. Add memory/update-time baselines for the per-library index and lint-cache implementation.
""",
)
replace_once(
    CHANGELOG,
    """- Persistent per-library import/export dependency fingerprints with deterministic affected-library
  evidence for downstream incremental consumers.
""",
    """- Persistent per-library import/export dependency fingerprints with deterministic affected-library
  evidence for downstream incremental consumers.
- Snapshot-backed and incremental lint contexts that retain unaffected per-library diagnostics while
  preserving full stateless lint equivalence.
""",
)
replace_once(
    UMBRELLA,
    """pub use dartscope_index::{
    DartIndexOptions, DartWorkspaceIndex, DartWorkspaceIndexCounters, DartWorkspaceSnapshot,
""",
    """pub use dartscope_index::{
    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceSnapshot,
""",
)

print("DS-INDEX-005 incremental lint context slice applied")
