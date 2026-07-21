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

No public Rust type, serialized field, reference kind, confidence rule, or index/parser boundary was
changed.

## Current Limits

The heuristic backend still defers:

- pattern and multi-declarator loop headers;
- collection control-flow elements;
- labels and local declarations used as an unbraced loop body;
- `try` statements and malformed nested control statements as unbraced loop bodies;
- retroactive pre-declaration shadowing, definite-assignment and flow analysis;
- member/index writes and destructuring.

## Next Ordered Slice

Continue `DS-INDEX-006` with conservative multi-declarator classic-loop bindings.

Required evidence before enabling the slice:

1. Per-declarator exact declaration spans and stable symbol IDs.
2. Initializer-order intervals where an earlier declarator can be read by a later initializer, but
   self and later-declarator references remain suppressed.
3. Condition, update, braced-body, and simple-statement-body read/write resolution fixtures.
4. Nearby negatives for patterns, destructuring, member/index targets, malformed headers, and
   collection control flow.
5. Parser/index parity proving that the index consumes only parser-produced facts.

## Verification Contract

Run the repository-pinned Rust 1.95.0 checks from `AGENTS.md`, including formatting, workspace tests,
Clippy with warnings denied, rustdoc with warnings denied, umbrella feature checks, and the hosted
Linux/Windows matrix. Do not mark this slice verified until the final feature commit publishes a
successful aggregate `dartscope/ci` status.
