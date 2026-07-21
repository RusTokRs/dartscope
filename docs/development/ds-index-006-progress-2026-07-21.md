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

No public Rust type, serialized field, reference kind, confidence rule, or index/parser boundary was
changed by either loop slice.

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

## Next Ordered Slice

Continue `DS-INDEX-006` with deterministic find-definition and find-references batch APIs over the
existing parser-produced namespace and lexical facts.

Required evidence before enabling the slice:

1. One reusable workspace resolution context shared by a batch of queries.
2. Stable deterministic ordering and exact source/declaration spans for every result.
3. Explicit unresolved, ambiguous, not-visible, and external-unindexed evidence rather than guessed
   targets.
4. Definition and reference queries for both namespace facts and lexical binding facts without raw
   source reparsing inside `dartscope-index`.
5. Full-rebuild and incremental-snapshot parity where the existing workspace index can provide the
   same normalized inputs.

## Verification Contract

Run the repository-pinned Rust 1.95.0 checks from `AGENTS.md`, including formatting, workspace tests,
Clippy with warnings denied, rustdoc with warnings denied, umbrella feature checks, and the hosted
Linux/Windows matrix. Do not mark the new slice verified until the final feature commit publishes a
successful aggregate `dartscope/ci` status.
