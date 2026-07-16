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

The low-level `to_json` and `to_json_pretty` helpers intentionally remain generic Serde helpers.
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
