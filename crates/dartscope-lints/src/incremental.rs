use std::collections::{BTreeMap, BTreeSet};

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
        let (diagnostics_by_library, global_diagnostics) = partition_analysis(snapshot, analysis);
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
                let analysis = lint_workspace_paths(snapshot, config, &local_rules, included_paths);
                self.diagnostics_by_library
                    .insert(owner.clone(), analysis.diagnostics);
            }
        }

        let global_rules_rebuilt = orphan_enabled(config) && workspace_update.rebuilt.uri_graph;
        if global_rules_rebuilt {
            self.global_diagnostics =
                lint_workspace_rules(snapshot, config, &[DartLintRuleId::OrphanFile]).diagnostics;
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
        let (diagnostics_by_library, global_diagnostics) = partition_analysis(snapshot, analysis);
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
        self.analysis =
            analysis_from_diagnostics(self.config.enabled_rule_ids().len(), diagnostics);
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

fn active_library_paths(snapshot: &DartWorkspaceSnapshot) -> BTreeMap<String, BTreeSet<String>> {
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
