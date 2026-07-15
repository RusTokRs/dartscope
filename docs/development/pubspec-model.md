---
id: doc://docs/development/pubspec-model.md
kind: implementation_note
language: en
status: active
---

# Structured Pubspec Model

`DS-PUB-002` is in progress. DartScope exposes two source-only pubspec APIs:

- `parse_pubspec` returns the primary complete model with package name, dependency sections, typed dependency sources, environment constraints, and common Flutter configuration;
- `parse_pubspec_configuration` returns the focused configuration analysis for callers and CLI smoke tests that do not need dependencies.

## Core Ownership

Dependency-source and configuration models live in `dartscope-core::pubspec`. This includes `PubspecDependencySource`, `PubspecConfiguration`, `PubspecConfigurationAnalysis`, environment constraints, Flutter assets, font families, and font assets.

Source normalization and the inherent `PubspecDependency::structured_source()` API also live in core. `dartscope-parse` keeps its previous root re-exports and `PubspecDependencySourceExt` as compatibility shims.

`PubspecDependency` stores a primary typed `source` field. The parser also emits the legacy `version_or_source` value during the pre-1.0 transition. Deserializing an older payload without `source` remains supported through a Serde default and `structured_source()` derives the typed value from the legacy field.

`PubspecAnalysis` stores a primary `configuration` field containing environment and Flutter configuration. Deserializing an older payload without `configuration` produces an empty default. Both direct `parse_pubspec` calls and pubspecs inside `analyze_project` use the same complete parser.

Checked-in JSON fixtures cover every dependency source variant, the focused environment/Flutter configuration shape, and the migrated complete `PubspecAnalysis` shape. Tests cover serialization round trips, typed-plus-legacy parser output, legacy-only dependency deserialization, and legacy analysis payloads without configuration.

## Parser Hardening

The complete parser now applies a private syntax-validation stage after dependency and configuration analysis:

- an unquoted dependency constraint of exactly `*` remains a valid wildcard and is not diagnosed as a YAML alias;
- named aliases such as `*defaults` remain explicitly unsupported;
- unmatched or mismatched flow delimiters and unterminated flow quotes produce a path-attributed `pubspec_invalid_yaml` diagnostic;
- invalid inline dependency mappings are removed from normalized output instead of being retained as fabricated dependency sources;
- nested flow mappings preserve quoted commas and YAML single-quote escaping;
- tab-indentation diagnostics do not desynchronize subsequent dependency validation.

This stage is transitional and will be removed after the marked-event YAML adapter provides the same behavior.

## YAML Backend

The maintained backend decision is accepted in [`yaml-backend.md`](yaml-backend.md):

- use `yaml-rust2` 0.11.x through a private marked-event adapter;
- initially pin `=0.11.0` with default features disabled;
- preserve byte evidence through `Marker::index` and reject aliases through explicit parser events;
- add the dependency and generated `Cargo.lock` update together only when the complete Rust 1.95.0 gates can run.

Deprecated `serde_yaml` and `serde_yml` are rejected. Other maintained candidates remain documented as alternatives rather than dependencies.

## Typed Configuration Output

`PubspecConfiguration` contains:

- `PubspecEnvironmentConstraint` values with exact key spans;
- `uses_material_design` and `generate_localizations` booleans;
- scalar Flutter asset paths and asset entries written as `path: ...` mappings;
- Flutter font families, asset paths, optional styles, and validated weights from 100 through 900.

The focused output remains available from the CLI:

```powershell
cargo run -p dartscope-cli -- pubspec-config path\to\pubspec.yaml
```

The primary migrated output is available through:

```powershell
cargo run -p dartscope-cli -- pubspec path\to\pubspec.yaml
```

## Compatibility Boundary

`version_or_source` remains serialized beside `source` until a versioned JSON contract defines its removal. New consumers should read `source` or call `structured_source()` rather than parsing the compatibility string.

The additive `configuration` field changes new `pubspec` and `analyze-project` JSON output. Older JSON remains readable because the field has a Serde default.

## Explicit Limitations

The current production implementation is still a conservative indentation-aware parser, not a complete YAML implementation. Aliases and merge keys remain unsupported by policy. Extended Flutter asset mappings such as `flavors` and `transformers` are diagnosed as unsupported rather than silently normalized. Flow-style environment and Flutter configuration mappings are not supported yet.

## Remaining Work

1. Add the pinned `yaml-rust2` dependency and lockfile update on Rust 1.95.0.
2. Implement the private marked-event adapter and require output parity with current fixtures.
3. Support extended Flutter asset mappings and any additional localization-owned fields justified by official Flutter documentation.
4. Run `cargo fmt`, Clippy, documentation, Linux tests, and Windows tests on Rust 1.95 before marking the task implemented or verified.
