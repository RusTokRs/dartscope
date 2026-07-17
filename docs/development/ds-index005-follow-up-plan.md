---
id: doc://docs/development/ds-index005-follow-up-plan.md
kind: development_plan
language: en
source_language: en
status: active
---

# DS-INDEX-005 Follow-up Plan

This plan records correctness findings discovered during the post-implementation audit of the
per-source URI and identifier-reference caches added in `1f458a598417b13192be7643d3d04c6ba83eaf78`.
DS-INDEX-005 remains `in progress`; the cache layer must not be promoted to `verified` until every
item below has a reproducible Rust 1.95 gate.

## P1: Same-name `NotVisible` evidence

The namespace resolver retains non-visible declaration candidates as evidence. A top-level declaration
rename or removal can therefore change the resolution output of a reference in a file with no import,
export, or part edge to the declaration file. Reverse URI traversal alone does not invalidate that
source path.

Required correction:

- compare resolution-relevant top-level declaration facts on file replacement or removal;
- collect affected old and new declaration names;
- invalidate every indexed reference source using one of those names;
- retain deterministic ordering and full-rebuild equivalence.

Regression fixture: a `Hidden()` invocation starts as `NotVisible`, its unrelated declaration is
renamed, and the cached result must become `Missing` with no retained candidates.

## P1: Sibling-part visibility

Changing a file's `part of` membership can change same-library visibility for references in sibling
parts. The reverse URI closure reaches the owner but does not necessarily reach every sibling part.

Required correction:

- construct old and new undirected components from matched owner/part links;
- when part links change, extend reference invalidation through both components;
- cover removal, mismatched `part of`, and movement between libraries.

Regression fixture: a declaration in `left.dart` is initially visible from sibling `right.dart`; after
`left.dart` leaves the owner library, the cached reference must become `NotVisible` and match a clean
rebuild.

## P1: Metadata paths in `affected_paths`

The first component-traversal prototype initialized its output with `changed_paths`. During global
package-resolution invalidation that echoed `pubspec.yaml` into the public Dart `affected_paths` list.
The API previously returned only the two affected Dart files.

Required correction:

- use changed paths only as traversal seeds/visited state;
- return only newly reached Dart owner/part paths;
- preserve the existing pubspec package-resolution fixture and public evidence shape.

## Verification Gate

The correction is complete only after all of the following pass on Rust 1.95.0:

1. focused fixtures for same-name evidence and sibling-part membership;
2. all `dartscope-index` tests, including the existing pubspec resolution fixture;
3. the deterministic 64-step incremental/full rebuild sequence;
4. the 1k/10k operation-count baseline;
5. workspace Clippy with warnings denied, rustdoc, workspace tests, umbrella all-features tests, and
   release-package validation;
6. a clean permanent Linux/Windows aggregate `dartscope/ci` result.

Temporary finalizer workflows and payloads are not part of the supported repository surface. If GitHub
Actions does not create a push run for connector-authored workflow changes, keep the permanent CI
read-only, leave these findings open, and retry through a reproducible clean-main verification path
rather than committing unverified cache semantics.
