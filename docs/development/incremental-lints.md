---
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
