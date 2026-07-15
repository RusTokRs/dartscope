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

The typed configuration API remains separate from `PubspecAnalysis`. Dependency sources are available through `PubspecDependencySourceExt::structured_source()`, while the legacy `version_or_source` field remains serialized for pre-1.0 compatibility.

Do not describe either bridge as the final core model. Moving dependency-source and configuration storage into `dartscope-core` requires an explicit JSON migration with golden fixtures.

## Explicit Limitations

The current implementation is a conservative indentation-aware parser, not a complete YAML implementation. It does not support aliases or merge keys. Extended Flutter asset mappings such as `flavors` and `transformers` are diagnosed as unsupported rather than silently normalized. Flow-style environment and Flutter configuration mappings are not supported yet.

## Remaining Work

1. Record the final maintained YAML backend decision for MSRV 1.85.
2. Move `PubspecDependencySource` and configuration types into `dartscope-core` as primary storage.
3. Define the compatibility and migration behavior for `version_or_source` and CLI JSON.
4. Support extended Flutter asset mappings and any additional localization-owned fields justified by official Flutter documentation.
5. Add checked-in serialization fixtures for every dependency and configuration variant.
6. Run `cargo fmt`, Clippy, documentation, Linux tests, and Windows tests before marking the task implemented or verified.
