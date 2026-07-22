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

## Current Limits

Direct member navigation remains intentionally bounded to parser-produced exact owner evidence.
Arbitrary receiver type inference, inherited-member traversal, extension selection, dynamic dispatch,
null-aware or cascade forms, patterns, and flow-sensitive behavior remain deferred. Compound index
assignment and increment/decrement semantics also remain deferred.

Unqualified instance references are not yet classified as same-owner member facts. A local variable,
parameter, local function, or imported/top-level declaration with the same spelling must continue to
win or suppress the member heuristic before that syntax can be resolved safely.

## Next Ordered Slice

Continue `DS-INDEX-006` with unqualified same-owner members inside an exact enclosing type:

1. Add bounded parser facts for unqualified `method()` calls, property reads, and property writes only
   when the enclosing callable supplies one exact owner symbol ID.
2. Suppress a member fact whenever a visible lexical binding, parameter, local function, or other
   exact non-member declaration shadows the spelling at that position.
3. Keep invocation, read, write, and static-versus-instance evidence explicit. Resolve only directly
   declared methods, fields, getters, and setters on the exact owner.
4. Preserve private-library behavior, validated parts, deterministic reverse references, missing-owner
   fallback, and full-build versus immutable-snapshot parity.
5. Keep inherited members, extension selection, arbitrary receiver inference, cascades, null-aware
   access, dynamic dispatch, patterns, and flow-sensitive behavior behind later focused slices.

## Verification Contract

Use the repository-pinned Rust 1.95.0 toolchain. A completed slice requires formatting, repository
consistency, workspace tests on Linux and Windows, macOS portability, Clippy and rustdoc with warnings
denied, umbrella feature and edition checks, bounded fuzzing, dependency audit and unused-dependency
checks, benchmark reporting, and a successful aggregate hosted CI result on the exact product head.
