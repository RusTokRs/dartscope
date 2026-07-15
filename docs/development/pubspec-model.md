---
id: doc://docs/development/pubspec-model.md
kind: implementation_note
language: en
status: active
---

# Structured Pubspec Model

`DS-PUB-002` is in progress. DartScope currently exposes two source-only pubspec APIs:

- `parse_pubspec` discovers the package name and dependency sections, preserves dependency-key spans, and normalizes scalar, SDK, path, git, hosted, workspace, and unknown dependency sources;
- `parse_pubspec_configuration` extracts typed environment constraints and common Flutter configuration.

## Core Ownership

Dependency-source and configuration models live in `dartscope-core::pubspec`. This includes `PubspecDependencySource`, `PubspecConfigurationAnalysis`, environment constraints, Flutter assets, font families, and font assets.

Source normalization and the inherent `PubspecDependency::structured_source()` API also live in core. `dartscope-parse` keeps its previous root re-exports and `PubspecDependencySourceExt` as compatibility shims, while `parse_pubspec_configuration` constructs core-owned configuration types directly.

`PubspecDependency` now stores a primary typed `source` field. The parser also emits the legacy `version_or_source` value during the pre-1.0 transition. Deserializing an older payload without `source` remains supported through a Serde default and `structured_source()` derives the typed value from the legacy field.

Checked-in JSON fixtures cover every dependency source variant and a complete environment/Flutter configuration example. Both fixtures verify serialization and deserialization round trips. An integration test covers typed-plus-legacy parser output and legacy-only deserialization.

## Typed Configuration Output

`PubspecConfigurationAnalysis` contains:

- normalized source path and path-attributed diagnostics;
- `PubspecEnvironmentConstraint` values with exact key spans;
- `uses_material_design` and `generate_localizations` booleans;
- scalar Flutter asset paths and asset entries written as `path: ...` mappings;
- Flutter font families, asset paths, optional styles, and validated weights from 100 through 900.

The same output is available from the CLI:

```powershell
cargo run -p dartscope-cli -- pubspec-config path\to\pubspec.yaml
```

## Compatibility Boundary

`version_or_source` remains serialized beside `source` until a versioned JSON contract defines its removal. New consumers should read `source` or call `structured_source()` rather than parsing the compatibility string.

The typed configuration API remains separate from `PubspecAnalysis`; embedding it is the remaining core-storage migration.

## Explicit Limitations

The current implementation is a conservative indentation-aware parser, not a complete YAML implementation. It does not support aliases or merge keys. Extended Flutter asset mappings such as `flavors` and `transformers` are diagnosed as unsupported rather than silently normalized. Flow-style environment and Flutter configuration mappings are not supported yet.

## Remaining Work

1. Record the final maintained YAML backend decision for MSRV 1.85.
2. Add primary configuration storage to `PubspecAnalysis` with an explicit CLI JSON migration.
3. Support extended Flutter asset mappings and any additional localization-owned fields justified by official Flutter documentation.
4. Add a checked-in fixture for the migrated complete pubspec shape.
5. Run `cargo fmt`, Clippy, documentation, Linux tests, and Windows tests before marking the task implemented or verified.
