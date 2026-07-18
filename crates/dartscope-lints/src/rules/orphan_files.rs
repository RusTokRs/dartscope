use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dartscope_core::{DartUriResolution, normalize_path};

use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLintConfig, DartLintDiagnostic, DartLintRuleId};

pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    let Some(uri_graph) = context.uri_graph() else {
        return;
    };
    if config.orphan_files.entry_points.is_empty() {
        return;
    }

    let indexed: BTreeSet<_> = context
        .project
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let roots: Vec<_> = config
        .orphan_files
        .entry_points
        .iter()
        .map(|path| normalize_path(path.clone()))
        .filter(|path| indexed.contains(path))
        .collect();
    if roots.is_empty() {
        return;
    }

    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for reference in &uri_graph.references {
        if reference.resolution != DartUriResolution::Resolved {
            continue;
        }
        let Some(target_path) = reference.target_path.as_ref() else {
            continue;
        };
        if indexed.contains(&reference.source_path) && indexed.contains(target_path) {
            adjacency
                .entry(reference.source_path.clone())
                .or_default()
                .insert(target_path.clone());
        }
    }

    let reachable = reachable_paths(&roots, &adjacency);
    let severity = config.severity(DartLintRuleId::OrphanFile);
    for path in indexed {
        if reachable.contains(&path)
            || config
                .orphan_files
                .ignored_path_prefixes
                .iter()
                .any(|prefix| path.starts_with(&normalize_path(prefix.clone())))
        {
            continue;
        }
        diagnostics.push(diagnostic(
            DartLintRuleId::OrphanFile,
            severity,
            "file is unreachable from configured lint entry points",
            path,
            None,
            roots.clone(),
        ));
    }
}

fn reachable_paths(
    roots: &[String],
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut reachable = BTreeSet::new();
    let mut queue: VecDeque<_> = roots.iter().cloned().collect();
    while let Some(path) = queue.pop_front() {
        if !reachable.insert(path.clone()) {
            continue;
        }
        if let Some(targets) = adjacency.get(&path) {
            queue.extend(targets.iter().cloned());
        }
    }
    reachable
}
