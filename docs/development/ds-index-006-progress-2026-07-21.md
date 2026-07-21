---
id: doc://docs/development/ds-index-006-progress-2026-07-21.md
kind: development_plan_update
language: en
status: active
---

# DS-INDEX-006 Progress Update — 2026-07-21

This update advances the ordered `DS-INDEX-006` work recorded in
`docs/development/dartscope-library-plan.md`. It is the current execution note for the next agent
cycle and must be consolidated into the main library plan when that document is next edited as a
whole.

## Completed Slice: Single-Statement Loop Scopes

Implemented on `main`:

1. Supported classic `for` loops with one simple non-block body statement. A loop declaration is
   visible in the condition, update, and body through one exact half-open binding interval.
2. Supported declared `for-in` variables with one simple body statement. The loop-local binding is
   visible only in that body.
3. Supported existing-variable `for-in` assignment targets with one simple body statement. The
   target emits one exact high-confidence `variable_write` and creates no new binding.
4. Preserved independent reads in classic initializers, conditions, updates, and `for-in` iterable
   expressions, plus existing plain-write and paired update behavior in the body.
5. Added conservative statement-boundary parsing for nested `if`, `for`, `while`, `switch`, and
   `do` statements so an unsupported nested control body remains one complete deferred region.
6. Kept labels, `try`, collection control flow, malformed statements, and bodies containing local
   declarations deferred rather than allowing a binding to leak beyond its real statement scope.
7. Added parser and index integration fixtures for exact spans, confidence, declaration and
   assignment targets, iterable/condition/update/body accesses, most-specific binding resolution,
   invocation-root filtering, namespace filtering, and nested-control negatives.
8. Filtered both bindings and explicit existing-variable `for-in` write targets when their
   declaration or target lies inside a deferred lexical region. A focused regression fixture prevents
   a supported nested target from leaking out of an unsupported outer loop region.
9. Updated the earlier braced-only parser and index fixtures to assert the later supported
   single-statement assignment target and paired body update instead of preserving stale negatives.

## Completed Slice: Multi-Declarator Classic Loops

Implemented on `main`:

1. Added conservative comma-separated classic-loop declarations with one exact declaration span,
   stable `for_variable` symbol ID, and half-open scope interval per declarator.
2. An initialized declarator becomes visible after its own initializer; an uninitialized declarator
   becomes visible immediately after its identifier. Earlier declarators can therefore be read or
   written from later initializers, while self and later-declarator accesses remain suppressed.
3. Reused the existing read, plain-write, combined-update, invocation-shadowing, and lexical-index
   paths for conditions, updates, braced bodies, and supported simple-statement bodies.
4. Kept comma-separated expression initializers, pattern/destructuring declarations, malformed
   continuation declarators, and unsupported body forms deferred instead of fabricating bindings.
5. Added parser and index fixtures for exact spans, stable IDs, initializer ordering, condition and
   update access, body resolution, namespace filtering, and parser/index parity.
6. Fixed whitespace normalization before `=` in continuation declarators; ordinary forms such as
   `second = first` no longer cause the complete loop to be deferred.
7. Suppressed invocation roots that refer to a self or later declarator inside the same declaration
   statement. The guard is bounded by statement delimiters, so a declaration still does not
   retroactively shadow an earlier independent statement.
8. Bounded lexical-read assignment lookahead at a top-level comma. An assignment in a later
   declarator can no longer suppress a valid parameter or earlier-binding read in the current
   initializer.
9. Applied the pinned Rust 1.95 formatter to every touched Rust file and removed the obsolete
   statement-boundary helper exposed by warnings during hosted verification.

## Completed Slice: Navigation Batch API Foundation

Implemented on `main`:

1. Added `DartWorkspaceResolutionContext`, which builds one URI graph, part-link view, namespace
   resolver, and lexical-resolution set for one normalized `DartProjectReferenceAnalysis`.
2. Added deterministic position-based `find_definitions` and `find_definitions_with_options` batches
   plus reusable-context methods. Queries are normalized, sorted, deduplicated, and resolved only from
   parser-produced reference and binding facts.
3. Unified namespace and lexical results through `DartDefinitionTarget` while retaining the original
   parser references, exact declaration spans, symbol IDs, namespace basis, and lexical intervals.
4. Added explicit `resolved`, `reference_missing`, `missing`, `ambiguous`, `not_visible`,
   `conditional_environment_required`, `external_unindexed`, and `source_file_missing` outcomes.
   External-unindexed results retain the relevant import URI evidence instead of guessing a target.
5. Added deterministic reverse `find_references` batches. Reverse lookup includes only facts whose
   definition is uniquely resolved to the selected target; ambiguous and not-visible facts are not
   attributed to a symbol.
6. Re-exported the API from `dartscope-index` and the umbrella `dartscope` crate without changing the
   command-facing v1 JSON envelopes.
7. Added integration fixtures for namespace and lexical definitions, paired lexical updates,
   not-visible and missing references, conditional imports, unindexed package imports, duplicate query
   elimination, stable ordering, reverse lookup, and stateless/reusable-context parity.
8. Kept all index work source-free: the context consumes normalized project/reference analysis and
   never reads or reparses raw Dart text.

## Completed Slice: Incremental Navigation Snapshot Parity

Implemented on `main`:

1. Retained normalized parser-produced identifier references and lexical bindings per path in
   `DartWorkspaceIndex` and aggregated them into every immutable `DartWorkspaceSnapshot`.
2. Stored the exact `DartIndexOptions` used for each snapshot and exposed source-free accessors for
   references, bindings, options, and a normalized `DartProjectReferenceAnalysis` projection.
3. Added `DartWorkspaceResolutionContext::from_snapshot`, reusing the snapshot URI graph, part-link
   analysis, compilation environment, project facts, references, and binding intervals.
4. Updated reference-aware file replacement to compare and replace bindings together with reference
   facts. Plain file replacement and file removal clear stale bindings for the affected path.
5. Preserved deterministic no-op behavior: an identical file/reference/binding update does not create
   a new generation or increment reference-rebuild counters.
6. Preserved old snapshot validity after later local updates and removals; navigation queries against
   an earlier generation retain their earlier project, reference, binding, URI, and option evidence.
7. Added full-build versus snapshot parity fixtures for initial construction, no-op replacement, local
   binding rename, declaration-file removal, and conditional-compilation option updates.
8. Kept the incremental boundary source-free. Neither the stateful index nor snapshot-backed
   navigation reads or reparses Dart source.

## Completed Slice: Exact Constructor Targets

Implemented on `main`:

1. Added a constructible-type namespace path that selects indexed classes and extension types without
   changing the existing general top-level symbol resolver.
2. Refined parser-produced `ConstructorTarget` facts from the owning type to exact unnamed or named
   `DartDeclarationKind::Constructor` candidates when the declaration inventory contains them.
3. Preserved the owning type as explicit fallback evidence for implicit unnamed constructors and for
   missing or parser-incomplete constructor declarations; missing named constructors and unavailable
   unnamed constructors remain explicit `missing` results.
4. Preserved prefixes, show/hide combinators, re-exports, conditional environments, part-library
   membership, ambiguity, not-visible candidates, and external-unindexed URI evidence.
5. Applied library-scoped privacy to private named constructors. The same private constructor resolves
   inside its owner library and remains `not_visible` through an import from another library.
6. Suppressed only a generic `InvocationTarget` fact that is identical to a specialized
   `ConstructorTarget` fact, preventing one explicit `new` or `const` expression from producing a false
   owner-plus-constructor ambiguity.
7. Added deterministic definition and reverse-reference fixtures for prefixed unnamed and named
   constructors, private and missing constructors, implicit unnamed fallback, ambiguous imports,
   conditional imports, part libraries, and external packages.
8. Kept the slice source-free and additive. It changes no serialized core field, reference kind,
   command envelope, or parser/index ownership boundary.

The loop slices did not change public Rust types or serialized fields. The navigation slices add
opt-in Rust library API types and snapshot accessors in `dartscope-index`; they do not alter stable
serialized command payloads, reference kinds, confidence rules, or parser/index ownership boundaries.

## Current Limits

The heuristic backend still defers:

- pattern and destructuring loop headers;
- comma-separated classic-loop expression initializers;
- collection control-flow elements;
- labels and local declarations used as an unbraced loop body;
- `try` statements and malformed nested control statements as unbraced loop bodies;
- retroactive pre-declaration shadowing across separate statements;
- definite-assignment and flow analysis;
- member/index writes, inherited-member lookup, and extension lookup.

Snapshot-backed navigation reconstructs the lightweight project-reference projection and resolution
context from immutable normalized facts. Exact constructor refinement currently consumes specialized
parser-produced `ConstructorTarget` facts; keyword-free constructor syntax remains on the generic
invocation path until constructor/member fact classification is broadened. Direct instance/static
members, inherited members, extensions, patterns, and flow-sensitive behavior remain deferred.

## Next Ordered Slice

Continue `DS-INDEX-006` with direct declared-member lookup:

1. Define parser-produced member facts that distinguish a member name from its receiver/owner evidence
   without treating import prefixes or constructor names as ordinary instance members.
2. Resolve directly declared methods, getters, setters, fields, and operators only when receiver or
   owning-type evidence is exact and indexed.
3. Preserve library privacy, parts, static-versus-instance evidence, ambiguity, not-visible outcomes,
   and external-unindexed URIs without claiming analyzer-equivalent type inference.
4. Keep inherited-member traversal, extension selection, dynamic dispatch, patterns, and flow-sensitive
   behavior behind later focused slices.
5. Add local, imported, prefixed, private, missing, ambiguous, part-library, and external member
   fixtures before expanding inheritance or extension lookup.

## Verification Contract

Run the repository-pinned Rust 1.95.0 checks from `AGENTS.md`, including formatting, workspace tests,
Clippy with warnings denied, rustdoc with warnings denied, umbrella feature checks, and the hosted
Linux/Windows matrix. Do not mark a new navigation sub-slice verified until its final feature commit
publishes a successful aggregate `dartscope/ci` status.
