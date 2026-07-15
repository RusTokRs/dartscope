---
id: doc://docs/development/pubspec-model.md
kind: implementation_note
language: en
status: active
---

# Structured Pubspec Model

`DS-PUB-002` is in progress. DartScope currently exposes two source-only pubspec APIs:

- `parse_pubspec` discovers the package name and dependency sections, preserves dependency-key spans, and normalizes scalar, SDK, path, git, hosted, workspace, and unknown dependency sources;
- `parse_pubspec_configuration` extracts typed environment constraints and common Flutter configuration without changing the pre-1.0 `PubspecAnalysis` JSON shape.

## Core Ownership

Dependency-source and configuration models now live in `dartscope-core::pubspec`. This includes `PubspecDependencySource`, `PubspecConfigurationAnalysis`, environment constraints, Flutter assets, font families, and font assets.

Source normalization and the inherent `PubspecDependency::structured_source()` API also live in core. `dartscope-parse` keeps its previous root re-exports and `PubspecDependencySourceExt` as compatibility shims, while `parse_pubspec_configuration` constructs the core-owned configuration types directly.

Checked-in JSON fixtures cover every dependency source variant and a complete environment/Flutter configuration example. Both fixtures verify serialization and deserialization round trips.

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

The typed configuration API remains separate from `PubspecAnalysis`. The legacy `version_or_source` field remains the serialized dependency-source storage field for pre-1.0 compatibility. The core constructor validates that its typed interpretation matches the normalized compatibility value in debug builds.

Do not describe the migration as complete until `PubspecDependency` and `PubspecAnalysis` store the typed source and configuration directly and the complete JSON transition is covered by golden fixtures.

## Explicit Limitations

The current implementation is a conservative indentation-aware parser, not a complete YAML implementation. It does not support aliases or merge keys. Extended Flutter asset mappings such as `flavors` and `transformers` are diagnosed as unsupported rather than silently normalized. Flow-style environment and Flutter configuration mappings are not supported yet.

## Remaining Work

1. Record the final maintained YAML backend decision for MSRV 1.85.
2. Add primary typed dependency-source and configuration storage to `PubspecAnalysis` with an explicit compatibility migration for `version_or_source` and CLI JSON.
3. Support extended Flutter asset mappings and any additional localization-owned fields justified by official Flutter documentation.
4. Add a checked-in fixture for the migrated complete pubspec shape.
5. Run `cargo fmt`, Clippy, documentation, Linux tests, and Windows tests before marking the task implemented or verified.
