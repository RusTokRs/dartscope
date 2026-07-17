---
id: doc://docs/development/symbol-resolution.md
kind: development_note
language: en
source_language: en
status: active
---

# General Symbol And Namespace Resolution

`dartscope-index` exposes a query API for resolving top-level Dart declarations through normalized
library namespaces. The resolver consumes `DartProjectAnalysis`; it performs no source parsing and no
filesystem I/O.

## Public API

- `resolve_symbol(&DartProjectAnalysis, DartSymbolQuery)` uses the default index options.
- `resolve_symbol_with_options(...)` accepts a `DartIndexOptions` compilation environment for
  conditional imports and exports.
- `DartSymbolQuery::with_prefix(...)` selects a prefixed import namespace.

Each result has an explicit status and retains deterministic declaration candidates with source path,
complete declaration span when available, declaration kind, symbol ID, and resolution basis.

## Resolution Order

For unprefixed queries the resolver applies Dart library precedence conservatively:

1. declarations in the same file;
2. declarations in another matched part of the same library;
3. declarations visible through direct imports and transitive re-exports.

Prefixed queries inspect only imports with the exact prefix. Import and export `show`/`hide`
combinators apply at every namespace edge. Private names remain visible only inside the same library.
Matched part files share their owner's import namespace.

## Explicit Outcomes

The first slice distinguishes:

- `resolved`;
- `missing`;
- `ambiguous`;
- `not_visible`;
- `conditional_environment_required`;
- `source_file_missing`.

Ambiguous and non-visible outcomes retain candidates rather than choosing one. Conditional imports or
exports require an explicit compilation environment before package semantics are selected.

## Current Boundary

The API resolves top-level declarations already present in `DartFileAnalysis`. It does not yet:

- discover identifier-use sites from arbitrary expressions;
- resolve members, constructors, extension lookup, types, or overload-like language behavior;
- resolve declarations from external packages that are absent from the loaded project index;
- replace the GraphQL-specific linker, which will migrate to the shared namespace engine in a
  follow-up DS-INDEX-004 slice.

These limits keep the first public contract deterministic and evidence-based.
