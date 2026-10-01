# Versioned JSON Contracts

DartScope command-facing JSON uses a stable envelope:

```json
{
  "schema": "dartscope.file-analysis",
  "version": 1,
  "data": {}
}
```

`schema` identifies one CLI command family, `version` is that schema's major contract
version, and `data` contains the serialized domain model. Consumers must dispatch on both
`schema` and `version` before interpreting `data`.

## Current contracts

- `dartscope.file-analysis` v1 — `analyze-file`
- `dartscope.pubspec-analysis` v1 — `pubspec`
- `dartscope.pubspec-configuration` v1 — `pubspec-config`
- `dartscope.project-analysis` v1 — `analyze-project`
- `dartscope.graphql-contracts` v1 — `graphql-contracts`
- `dartscope.uri-graph` v1 — `uri-graph`
- `dartscope.flutter-inventory` v1 — `flutter-inventory`
- `dartscope.lint-analysis` v1 — `lint --format json`

## Compatibility policy

The envelope fields and their meanings are fixed for a schema version. The following changes
are additive and may remain on the same version when documented and covered by updated golden
fixtures:

- adding a nullable or optional field to `data`;
- adding a new enum value where consumers are already required to handle unknown values;
- adding a new diagnostic code;
- adding list entries while retaining documented deterministic ordering.

The following changes require a new major version for the affected schema:

- removing or renaming a field;
- changing a field's JSON type or nullability;
- changing an enum representation or removing a value;
- changing path, span, diagnostic, or ordering semantics;
- moving a command to a different payload model.

A schema version bump keeps the old fixture in the repository, adds a new fixture, and records
migration guidance below. CLI commands never emit an unversioned payload.

SARIF output is independently versioned by the SARIF `2.1.0` document and schema URI rather than by
a DartScope envelope. The low-level `to_json` and `to_json_pretty` helpers intentionally remain generic Serde helpers.
Their raw output is not a stable DartScope schema and must not be used for command-facing JSON.

## Deterministic output

Domain producers sort paths and findings before serialization. Contract fixtures use struct field
order and deterministic sequence order; consumers must not rely on JSON object member order.
Golden tests run on the standard Linux and Windows matrix.

## Migration history

- Initial release: `dartscope.file-analysis` v1.
- Initial release: `dartscope.pubspec-analysis` v1.
- Initial release: `dartscope.pubspec-configuration` v1.
- Initial release: `dartscope.project-analysis` v1.
- Initial release: `dartscope.graphql-contracts` v1.
- Initial release: `dartscope.uri-graph` v1.
- Initial release: `dartscope.flutter-inventory` v1.
- `0.2` addition: `dartscope.lint-analysis` v1.

## Additive v1 migration: generic invocations and optional Flutter composition

`dartscope.file-analysis` v1 and `dartscope.project-analysis` v1 now include an optional
`invocations` list on each Dart file. Older payloads omit the list and deserialize it as empty.
Each invocation records a generic target, arguments, optional result members, optional enclosing
symbol ID, and source evidence; Flutter-specific meaning is not stored in these records.

The existing `flutter` field remains in `DartFileAnalysis` for v1 compatibility. Pure
`dartscope-parse` APIs leave that projection empty. The Flutter-aware umbrella composition APIs and
the CLI populate it from generic facts, so existing CLI consumers retain widget, route, asset, and
localization output without a schema-major change. `dartscope-flutter` also accepts older payloads
that contain legacy Flutter hints but no invocation facts.

## Additive v1 migration: Flutter asset and localization catalogs

`dartscope.flutter-inventory` v1 can now include optional `asset_declarations`,
`l10n_configurations`, `arb_catalogs`, and `diagnostics` lists. Asset entries can include an
optional literal package, unresolved package expression, and declaration link. Localization
entries can include the generated
class and ARB catalog paths. Empty additions are omitted and older payloads deserialize them as
empty or absent.

`DartDiagnostic` also accepts an optional confidence value. Existing diagnostics omit it; new
Flutter catalog diagnostics use it to distinguish exact literal mismatches from partial-input or
dynamic-use uncertainty. No existing field was removed, renamed, or assigned new span semantics.

## v1 corrections from the 2026-09-30 audit

The audit found values that the parser reported wrongly. They are corrected inside v1, without a
schema-major bump, because no field was removed, renamed or retyped and no span semantics changed;
a consumer that relied on an old value has to adapt:

- `declarations[].extends` is the `extends` type of a class and nothing else. It used to hold the `on`
  type of an extension.
- `declarations[].mixes_in` holds the `with` types of classes and enums. It used to hold the `on`
  constraints of a mixin.
- New optional `declarations[].on_types` (omitted when empty; older payloads deserialize it as empty)
  holds the `on` constraints of a mixin and the `on` type of an extension. It is empty for an extension
  whose `on` type is one of its own type parameters (`extension X<T> on T`), which applies to every
  receiver. The golden fixtures contain no `extension` or `mixin`, so they did not change.
- Enum constants are reported as `field` declarations whose parent is the enum, and a top-level `get` or
  `set` is reported as a `getter` or `setter`; both were missing before.
- A leading UTF-8 byte-order mark is a preamble: lines and columns do not count it, byte offsets stay
  absolute, and the first line of the file is scanned like any other.
- `analyze-project` can report the new warning `input_file_not_utf8` (with the file path) for a `.dart`
  file that is not valid UTF-8; the file is not part of `files`.
- `uri-graph`: percent escapes in a relative `import`, `export` or `part` URI are decoded. A relative
  URI that is empty or blank, contains an escaped path separator or climbs out of the project root, and
  a `package:` URI that climbs out of its library directory, resolve to `invalid_uri` without a
  `target_path`; they used to be `missing_target` with a path, or a path with the surplus `..` segments
  dropped (a file inside the root that the URI does not mean).

## Opt-in reference analysis outside command v1 payloads

Identifier-reference wrappers and batch namespace-resolution results are library APIs rather than new
fields on `dartscope.file-analysis` v1 or `dartscope.project-analysis` v1. Existing CLI envelopes and
golden fixtures therefore remain unchanged. A future command-facing reference contract must receive
its own registered schema name and golden fixture before release.
