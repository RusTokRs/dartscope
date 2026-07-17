---
id: doc://docs/development/incremental-index.md
kind: development_contract
language: en
source_language: en
status: active
---

# Incremental Workspace Index

`DartWorkspaceIndex` is the stateful boundary for repeatedly indexing already normalized DartScope
analysis models. It performs no filesystem reads, starts no Dart or Flutter process, and never exposes
or retains parser ASTs.

The existing stateless functions remain the semantic reference implementation. Every snapshot product
is built by those same URI, part-link, namespace, GraphQL, and identifier-reference algorithms.

## Ownership And Thread Safety

- Mutations require `&mut DartWorkspaceIndex`. The index contains no hidden lock and callers choose the
  synchronization policy appropriate for their application.
- `snapshot()` returns `Arc<DartWorkspaceSnapshot>`. A retained snapshot is immutable and remains valid
  after the mutable index advances to later generations.
- The index and snapshot types are `Send + Sync`; mutation is still serialized by Rust's `&mut` rule.
- Inputs are normalized public analysis models such as `DartFileAnalysis`, `PubspecAnalysis`, and
  `PackageConfigAnalysis`, not source text or backend-specific syntax trees.

## Stateful API

```rust
let mut index = DartWorkspaceIndex::from_reference_project(project_analysis);
let first = index.snapshot();

let update = index.upsert_file_with_references(changed_file_analysis);
let second = index.snapshot();

assert_eq!(first.generation(), 0);
assert_eq!(second.generation(), update.generation);
```

The mutation surface includes:

- `upsert_file` and `upsert_file_with_references`;
- `remove_file`;
- `upsert_pubspec` and `remove_pubspec`;
- `upsert_package_config` and `remove_package_config`;
- `update_options` for conditional-compilation configuration;
- `update_root` for the informational project root.

Calling `upsert_file` without reference facts intentionally clears old reference facts for that path.
Use `upsert_file_with_references` when the parser reference capability is enabled.

## Snapshots

One snapshot owns deterministic projections for:

- the canonical `DartProjectAnalysis` and summary;
- the URI graph;
- part links;
- GraphQL operation bindings;
- opt-in identifier-reference resolutions.

Unchanged products are retained through shared `Arc` storage. Public getters expose immutable model
references rather than implementation caches.

## Invalidation And Counters

`DartWorkspaceUpdate` reports normalized changed paths, the transitive reverse dependency closure, and
which products were rebuilt. Reverse dependencies include resolved targets, missing target paths, and
ambiguous package candidates from both the old and new URI graph.

The first implementation reuses products at subsystem granularity:

| Change | Project | URI graph | Part links | GraphQL | References |
| --- | --- | --- | --- | --- | --- |
| diagnostics, Flutter hints, or string inventory only | rebuild | reuse | reuse | reuse | reuse |
| imports or exports | rebuild | rebuild | reuse | rebuild | rebuild |
| part directives or library membership | rebuild | as needed | rebuild | rebuild | rebuild |
| declarations only | rebuild | reuse | reuse | reuse | rebuild |
| GraphQL operations or uses only | rebuild | reuse | reuse | rebuild | reuse |
| reference facts only | reuse | reuse | reuse | reuse | rebuild |
| compilation environment | reuse | rebuild | reuse | rebuild | rebuild |
| package-resolution metadata | rebuild | rebuild | rebuild | rebuild | rebuild |

`DartWorkspaceIndexCounters` records generations, no-op updates, and rebuild counts. These are
semantic operation counters rather than elapsed-time assertions, so they are deterministic across
Linux, Windows, and differently loaded runners.

Run the synthetic 1k/10k-file operation baseline with:

```text
cargo run -p dartscope-index --example incremental_workspace_baseline --release
```

## Equivalence Contract

After every mutation, the snapshot project and derived outputs must equal a clean stateless rebuild over
the same normalized inputs and `DartIndexOptions`. Tests cover replacement, removal, package metadata,
conditional environments, paths with Windows separators, retained snapshots, no-op updates, and reverse
closure ordering.

## Current Boundary

This foundation reports the exact reverse closure and avoids unrelated subsystem rebuilds. A later
DS-INDEX-005 slice will move URI, namespace, GraphQL, and reference storage from subsystem-level reuse to
per-file and per-library caches, then expose the same invalidation evidence to lint contexts. The public
stateful API and existing stateless APIs are intended to remain stable while that internal granularity
improves.
