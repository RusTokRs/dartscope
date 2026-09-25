---
id: doc://docs/development/ds-index-006-progress-2026-07-22.md
kind: development_plan_update
language: en
status: active
---

# DS-INDEX-006 Progress Update — 2026-07-22

This note supersedes `ds-index-006-progress-2026-07-21.md` as the current execution note while
preserving that file as the historical record for the earlier loop, navigation, constructor, method,
property, audit, and binary-operator slices.

## Completed Audit: Constructor And Direct Member Navigation

The constructor, method, and property slices were rechecked with new regression fixtures. The audit
found and corrected observable errors rather than only documenting risks:

1. Member declaration facts now use kind-aware name extraction. A parameter, initializer expression,
   or repeated identifier later in the header can no longer steal the declaration anchor.
2. Private named-type roots such as `_Service` are recognized when exact owner evidence exists, while
   lexical bindings still suppress named-type reinterpretation.
3. Static owner resolution covers classes, mixins, enums, extensions, and extension types instead of
   reusing the constructor-only owner filter.
4. Direct method tear-offs and callable field/getter values resolve before constructor fallback.
5. Exact constructor tear-offs and keyword-free named constructor calls use the constructor path only
   when no directly declared member survives.
6. Member inventory and refinement were extracted from the oversized navigation root into
   `navigation/members.rs` before further feature work.
7. Cross-platform fixtures cover the corrected declaration spans, owner kinds, private owners,
   callable values, tear-offs, constructor calls, parts, reverse references, and snapshot parity.

## Completed Slice: Explicit-`this` Operator Targets

Implemented on `main`:

1. Parser-owned declaration facts retain the exact `DartDeclarationKind::Operator` token, owner symbol
   ID, enclosing callable evidence, span, and confidence.
2. Bounded invocation facts cover overloadable binary operators whose left receiver is directly
   `this`, unary `-this` and `~this`, `this[index]`, and plain `this[index] = value`.
3. Binary classification requires `this` to start the operand expression, preventing a later token in
   `other + this + value` from being attributed to the operator declared by `this`'s owner.
4. Index reads resolve to `operator []`; plain index assignments resolve to `operator []=`. Their
   position fact anchors the opening bracket while retaining the normalized operator name separately.
5. Compound index updates and index increment/decrement remain deferred rather than fabricating a
   single read or write target with incomplete evaluation semantics.
6. Source-free member refinement resolves only directly declared exact-owner operator candidates.
   Missing overloads preserve the owner as explicit fallback evidence.
7. Definition lookup, reverse references, validated part libraries, and immutable snapshot parity are
   covered by focused parser and index fixtures.
8. No serialized field or command-facing v1 envelope changed. The slice extends only the existing
   opt-in reference analysis behavior.

## Completed Slice: Unqualified Same-Owner Members

The ordered slice from the previous update is implemented on the branch:

1. Parser-side `unqualified_member_references` classifies an unqualified spelling as a member fact only
   when the enclosing callable supplies one exact owner symbol ID and that owner directly declares a
   matching method, field, getter, or setter. Calls, reads, and writes emit the existing
   `MemberInvocation*`/`MemberProperty*` static-or-instance kinds with high confidence and exact spans;
   no new public reference kind or serialized field was added.
2. Suppression is explicit: `this`/`super` roots stay excluded, and visible parameters, block-locals,
   import prefixes, enclosing-owner members, and local function declarations always win or suppress the
   member heuristic, including a declaration-shaped local function scanned inside the masked callable
   body.
3. Compound assignment and increment targets emit the paired read-then-write facts for a directly
   declared field instead of one fabricated target; a write to a getter-only or method member, or a read
   of a setter-only member, is suppressed rather than guessed.
4. Index resolution reuses the directly declared exact-owner member inventory: static shortcuts keep
   working for `::` owner prefixes, private-library visibility and validated parts are preserved, and
   missing members retain the owner fallback evidence.
5. Focused fixtures cover resolution kinds, same-file evidence, missing members, incremental versus
   full-build parity, and rename invalidation
   (`crates/dartscope-parse/tests/unqualified_member_references.rs`,
   `crates/dartscope-index/tests/navigation_unqualified_members.rs`).

## Current Limits

Direct member navigation remains intentionally bounded to parser-produced exact owner evidence.
Arbitrary receiver type inference, inherited-member traversal, extension selection, dynamic dispatch,
null-aware or cascade forms, patterns, and flow-sensitive behavior remain deferred. Compound index
assignment and increment/decrement semantics also remain deferred.

Unqualified member evidence is now produced safely, but only for directly declared members of the exact
enclosing owner. Inherited members, extension members, and implicit constructor selection still need
their own focused slices; local functions are also not modeled as lexical bindings yet, so the guard
only suppresses member evidence when it recognizes a declaration-shaped local function.

## Next Ordered Slice

Continue `DS-INDEX-006` with the next bounded, evidence-gated slice: inherited-member or extension
selection for an exact owner type, or local-function binding modeling. Each slice must arrive with
nearby-shadowing fixtures, exact spans, an explicit compatibility note, and full-build versus
immutable-snapshot parity before it enters public output.

## Verification Contract

Use the repository-pinned Rust 1.95.0 toolchain. A completed slice requires formatting, repository
consistency, workspace tests on Linux and Windows, macOS portability, Clippy and rustdoc with warnings
denied, umbrella feature and edition checks, bounded fuzzing, dependency audit and unused-dependency
checks, benchmark reporting, and a successful aggregate hosted CI result on the exact product head.
