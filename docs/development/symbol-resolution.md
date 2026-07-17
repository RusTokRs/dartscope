---
id: doc://docs/development/symbol-resolution.md
kind: development_note
language: en
source_language: en
status: active
---

# General Symbol And Namespace Resolution

`dartscope-index` resolves top-level Dart declarations through normalized library namespaces. The
resolver consumes `DartProjectAnalysis`; it performs no source parsing and no filesystem I/O.

## Public Query API

- `resolve_symbol(&DartProjectAnalysis, DartSymbolQuery)` uses the default index options.
- `resolve_symbol_with_options(...)` accepts a `DartIndexOptions` compilation environment for
  conditional imports and exports.
- `DartSymbolQuery::with_prefix(...)` selects a prefixed import namespace.

Each result has an explicit status and retains deterministic declaration candidates with source path,
complete declaration span when available, declaration kind, symbol ID, and resolution basis.

## Opt-In Reference Analysis

`dartscope-parse` also exposes `analyze_file_with_references` and
`analyze_project_with_references`. These return the normal analysis plus conservative
`DartIdentifierReference` facts. The first bounded model records invocation-target roots only:

- an exact import prefix plus invoked declaration name, with high confidence;
- an unqualified invocation root, with medium confidence;
- exact identifier span and optional enclosing symbol ID.

Comments, strings, declaration-header calls, member tails, and duplicate roots from a chained
invocation are not emitted. The model intentionally does not claim every identifier token is a
semantic reference.

`dartscope-index` resolves these facts in one reusable namespace context through:

- `resolve_identifier_references`;
- `resolve_identifier_references_with_options`;
- `resolve_project_identifier_references`;
- `resolve_project_identifier_references_with_options`.

The batch API consumes normalized facts and never reads source text.

## Resolution Order

For unprefixed queries the resolver applies Dart library precedence conservatively:

1. declarations in the same file;
2. declarations in another matched part of the same library;
3. declarations visible through direct imports and transitive re-exports.

Prefixed queries inspect only imports with the exact prefix. Import and export `show`/`hide`
combinators apply at every namespace edge. Private names remain visible only inside the same library.
Matched part files share their owner's import namespace.

## Explicit Outcomes

The resolver distinguishes:

- `resolved`;
- `missing`;
- `ambiguous`;
- `not_visible`;
- `conditional_environment_required`;
- `source_file_missing`.

Ambiguous and non-visible outcomes retain candidates rather than choosing one. Conditional imports or
exports require an explicit compilation environment before package semantics are selected.

The GraphQL contract analyzer uses the same internal namespace engine for operation-constant
visibility while retaining its existing public binding, unresolved-reason, candidate-path, and
ordering contracts.

## JSON Compatibility

Reference wrappers and batch-resolution models are opt-in library APIs. The seven command-facing v1
JSON payloads continue to serialize the existing `DartFileAnalysis` and `DartProjectAnalysis`
models, so this slice does not add fields to their stable envelopes or require golden changes.

## Current Boundary

The current reference producer does not yet:

- discover arbitrary variable reads, assignments, type annotations, or pattern references;
- resolve members, constructors as members, extension lookup, types, or overload-like behavior;
- resolve declarations from external packages absent from the loaded project index;
- model lexical shadowing beyond the conservative invocation-target confidence boundary.

These limits keep the first batch contract deterministic and evidence-based.
