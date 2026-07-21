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

The loop slices did not change public Rust types or serialized fields. The navigation slice adds
opt-in Rust library API types in `dartscope-index`; it does not alter stable serialized command
payloads, reference kinds, confidence rules, or parser/index ownership boundaries.

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

The stateful workspace index does not yet retain parser-produced lexical bindings in snapshots, so
navigation parity is currently available from a full `DartProjectReferenceAnalysis` context only.

## Next Ordered Slice

Continue `DS-INDEX-006` with incremental navigation parity:

1. Retain normalized lexical bindings per path in `DartWorkspaceIndex` and immutable snapshots.
2. Rebuild only affected navigation facts after reference-file updates while preserving old snapshot
   validity and deterministic counters.
3. Expose a snapshot-backed `DartWorkspaceResolutionContext` or equivalent batch entrypoint with the
   same full-rebuild definition/reference results.
4. Add full-build versus no-op, local-update, declaration-update, removal, and options-update parity
   fixtures.
5. Preserve the current source-free boundary and explicit unresolved/external evidence.

After parity, continue the remaining `DS-INDEX-006` lookup slices for constructor/member/extension
resolution and the still-deferred pattern/reference forms.

## Verification Contract

Run the repository-pinned Rust 1.95.0 checks from `AGENTS.md`, including formatting, workspace tests,
Clippy with warnings denied, rustdoc with warnings denied, umbrella feature checks, and the hosted
Linux/Windows matrix. Do not mark a new navigation sub-slice verified until its final feature commit
publishes a successful aggregate `dartscope/ci` status.
