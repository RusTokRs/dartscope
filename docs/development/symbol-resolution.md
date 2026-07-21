---
id: doc://docs/development/symbol-resolution.md
kind: development_note
language: en
source_language: en
status: active
---

# General Symbol And Namespace Resolution

`dartscope-index` resolves top-level Dart declarations through normalized library namespaces and
parser-produced lexical bindings through exact half-open intervals. The resolvers consume normalized
analysis models; they perform no source parsing and no filesystem I/O.

## Public Symbol Query API

- `resolve_symbol(&DartProjectAnalysis, DartSymbolQuery)` uses the default index options.
- `resolve_symbol_with_options(...)` accepts a `DartIndexOptions` compilation environment for
  conditional imports and exports.
- `DartSymbolQuery::with_prefix(...)` selects a prefixed import namespace.

Each result has an explicit status and retains deterministic declaration candidates with source path,
complete declaration span when available, declaration kind, symbol ID, and resolution basis.

## Opt-In Reference Analysis

`dartscope-parse` exposes `analyze_file_with_references` and
`analyze_project_with_references`. These return normal analysis plus conservative
`DartIdentifierReference` and `DartLexicalBinding` facts for the currently supported namespace,
type-position, constructor, parameter, local-variable, closure, catch, and loop slices.

Comments, strings, unsupported member/index targets, and deferred syntax regions do not receive
fabricated semantic facts. The model intentionally does not claim every identifier token is a
reference.

`dartscope-index` resolves normalized namespace facts through:

- `resolve_identifier_references`;
- `resolve_identifier_references_with_options`;
- `resolve_project_identifier_references`;
- `resolve_project_identifier_references_with_options`.

It resolves lexical facts through:

- `resolve_project_lexical_binding`;
- `resolve_project_variable_read_references`;
- `resolve_project_variable_write_references`.

These APIs consume normalized facts and never read source text.

## Navigation Batch API

`DartWorkspaceResolutionContext` prepares one reusable URI graph, part-link view, namespace resolver,
and lexical-resolution set from a `DartProjectReferenceAnalysis`. The following APIs use that shared
context:

- `find_definitions` and `find_definitions_with_options` for one deterministic batch of normalized
  path/byte-offset queries;
- `DartWorkspaceResolutionContext::find_definitions` for repeated batches without rebuilding the
  workspace context;
- `find_references`, `find_references_with_options`, and the corresponding context method for reverse
  lookup from selected namespace or lexical definition targets.

A definition result retains all parser facts covering the query position, exact namespace or lexical
target evidence, and relevant external import URIs. Reverse lookup attributes a reference only when
its definition is uniquely resolved to the requested target; ambiguous and not-visible facts remain
unattributed.

## Resolution Order

For unprefixed namespace queries the resolver applies Dart library precedence conservatively:

1. declarations in the same file;
2. declarations in another matched part of the same library;
3. declarations visible through direct imports and transitive re-exports.

Prefixed queries inspect only imports with the exact prefix. Import and export `show`/`hide`
combinators apply at every namespace edge. Private names remain visible only inside the same library.
Matched part files share their owner's import namespace.

Lexical queries select the most specific visible binding interval, using the exact source path, name,
position, and optional enclosing-symbol evidence emitted by the parser.

## Explicit Outcomes

Navigation definition batches distinguish:

- `resolved`;
- `reference_missing`;
- `missing`;
- `ambiguous`;
- `not_visible`;
- `conditional_environment_required`;
- `external_unindexed`;
- `source_file_missing`.

Ambiguous and non-visible outcomes retain candidates rather than choosing one. Conditional imports or
exports require an explicit compilation environment before package semantics are selected.
External-unindexed outcomes retain relevant import URIs without inventing declaration candidates.

The GraphQL contract analyzer uses the same internal namespace engine for operation-constant
visibility while retaining its existing public binding, unresolved-reason, candidate-path, and
ordering contracts.

## JSON Compatibility

Reference wrappers, navigation contexts, and batch-resolution models are opt-in Rust library APIs.
The seven command-facing v1 JSON payloads continue to serialize the existing `DartFileAnalysis` and
`DartProjectAnalysis` models, so this slice does not add fields to their stable envelopes or require
golden changes.

## Current Boundary

The current reference and navigation producers do not yet:

- retain lexical bindings in incremental workspace snapshots for full-build/incremental navigation
  parity;
- resolve member/index writes, inherited members, extension lookup, or overload-like behavior;
- cover deferred pattern/destructuring and collection-control-flow forms;
- resolve declarations from external packages absent from the loaded project index;
- perform definite-assignment or flow-sensitive analysis.

These limits keep the navigation contract deterministic and evidence-based.
