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
- stable per-library import/export dependency fingerprints;
- GraphQL operation bindings;
- opt-in identifier-reference resolutions.

Unchanged products are retained through shared `Arc` storage. Public getters expose immutable model
references rather than implementation caches.

## Invalidation And Counters

`DartWorkspaceUpdate` reports normalized changed paths, the transitive reverse dependency closure,
normalized affected library owners, and which products were rebuilt. Reverse dependencies include
resolved targets, missing target paths, and ambiguous package candidates from both the old and new URI
graph. Part paths collapse to their matched owner in `affected_libraries`; metadata paths are excluded.

The current implementation combines immutable subsystem snapshots with per-file URI-reference and
identifier-resolution caches:

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

`DartWorkspaceIndexCounters` records generations, aggregate rebuilds, the exact number of URI and
identifier-reference source files recomputed, and the number of namespace-membership, dependency-
fingerprint, and GraphQL-use libraries refreshed. Unaffected per-file and per-library `Arc` cache entries
remain shared internally.
These are semantic operation counters rather than elapsed-time assertions, so they are deterministic
across Linux, Windows, and differently loaded runners.

A local reference-fact replacement invalidates only its source path. File insertion/removal recomputes
that path plus direct URI sources whose previous target resolution may change. Namespace changes report
the transitive reverse closure. Top-level declaration changes additionally invalidate every reference
source using an affected name because retained `NotVisible` evidence can change without an import edge.
Changes to part membership also traverse old and new matched owner/part components so sibling-part
visibility stays equivalent to a clean rebuild. Metadata paths themselves are not emitted as Dart
`affected_paths` by this component traversal.

Run the synthetic 1k/10k-file operation baseline with:

```text
cargo run -p dartscope-index --example incremental_workspace_baseline --release
```

## Equivalence Contract

After every mutation, the snapshot project and derived outputs must equal a clean stateless rebuild over
the same normalized inputs and `DartIndexOptions`. Tests cover replacement, removal, package metadata,
conditional environments, paths with Windows separators, retained snapshots, no-op updates, reverse
closure ordering, same-name `NotVisible` evidence outside the URI graph, sibling-part visibility,
and a deterministic 64-step mixed update sequence.

## Current Boundary

URI references and identifier-reference resolutions use per-source-file caches. Library membership,
import/export dependency fingerprints, and GraphQL bindings use retained per-library caches. Snapshots
publish deterministic fingerprints without exposing mutable cache storage, and updates publish normalized
affected-library owners consumed by `DartIncrementalLintCache`. The dependency remains one-way from the
optional lint crate to the index crate, and the public stateless APIs remain available.
