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

## Execution Checklist

Completed implementation slices are marked independently from checks that still require an
executable Rust 1.95.0 environment.

- [x] Move dependency-source and pubspec-configuration contracts into `dartscope-core`.
- [x] Preserve legacy `version_or_source`, `assets`, and missing-configuration JSON inputs.
- [x] Normalize version, SDK, path, git, hosted, workspace, and fallback dependency sources.
- [x] Preserve dependency-key and environment-key source spans.
- [x] Harden wildcard-versus-alias handling and malformed dependency flow mappings.
- [x] Normalize Flutter booleans, fonts, scalar assets, and `path` asset mappings.
- [x] Add `flavors`, `platforms`, ordered transformers, scalar args, and compatibility fixtures.
- [x] Audit extended assets for colon-containing scalars, invalid scalar metadata,
  inconsistent transformer indentation, and nested-mode leakage; add regression tests.
- [x] Accept one explicit YAML document, ignore additional documents, diagnose duplicate
  top-level/direct mapping keys, and preserve CRLF/Unicode byte evidence.
- [x] Preserve non-empty flavor names as application-defined values, validate asset
  platforms against Flutter's six documented values, and keep richer localization
  configuration outside the pubspec-owned `generate` field.
- [x] Select and document `yaml-rust2` 0.11.x as the private marked-event backend.
- [ ] Add `yaml-rust2 = "=0.11.0"` with default features disabled and regenerate
  `Cargo.lock` using Rust 1.95.0.
- [ ] Implement the private marked-event adapter and prove normalized-output parity with
  the current fixtures before switching the public parser.
- [ ] Run formatting, focused tests, workspace tests, Clippy, rustdoc, Linux/Windows tests,
  and the edition-2024 matrix on Rust 1.95.0.

## Core Ownership

Dependency-source and configuration models live in `dartscope-core::pubspec`. This includes `PubspecDependencySource`, `PubspecConfiguration`, `PubspecConfigurationAnalysis`, environment constraints, Flutter asset configurations and transformers, font families, and font assets.

Source normalization and the inherent `PubspecDependency::structured_source()` API also live in core. `dartscope-parse` keeps its previous root re-exports and `PubspecDependencySourceExt` as compatibility shims.

`PubspecDependency` stores a primary typed `source` field. The parser also emits the legacy `version_or_source` value during the pre-1.0 transition. Deserializing an older payload without `source` remains supported through a Serde default and `structured_source()` derives the typed value from the legacy field.

`PubspecAnalysis` stores a primary `configuration` field containing environment and Flutter configuration. Deserializing an older payload without `configuration` produces an empty default. Both direct `parse_pubspec` calls and pubspecs inside `analyze_project` use the same complete parser.

Checked-in JSON fixtures cover every dependency source variant, the focused environment/Flutter configuration shape, structured Flutter asset mappings, and the migrated complete `PubspecAnalysis` shape. Tests cover serialization round trips, typed-plus-legacy parser output, legacy-only dependency deserialization, legacy analysis payloads without configuration, and older Flutter configuration payloads without extended asset fields.

## Parser Hardening

The complete parser now applies a private syntax-validation stage before dependency and configuration analysis:

- an unquoted dependency constraint of exactly `*` remains a valid wildcard and is not diagnosed as a YAML alias;
- named aliases such as `*defaults` remain explicitly unsupported;
- unmatched or mismatched flow delimiters and unterminated flow quotes produce a path-attributed `pubspec_invalid_yaml` diagnostic;
- invalid inline dependency mappings are removed from normalized output instead of being retained as fabricated dependency sources;
- nested flow mappings preserve quoted commas and YAML single-quote escaping;
- tab-indentation diagnostics do not desynchronize subsequent dependency validation;
- optional leading `---` and trailing `...` markers are blanked without changing source length, while a second document is diagnosed and excluded from both public parser paths;
- duplicate top-level keys and duplicate direct keys in dependency, environment, and Flutter mappings produce `pubspec_duplicate_key` diagnostics with exact key spans.

The structured asset stage additionally distinguishes mapping separators from colons inside plain scalars, rejects metadata attached to scalar list entries, checks sibling/nested indentation, and resets list modes at mapping boundaries. Its YAML-subset primitives are isolated in `pubspec_yaml_subset.rs` so they can be removed with the marked-event adapter.

Asset selector validation follows the official Flutter boundary:

- flavor names are application-defined and remain opaque, but empty names are invalid;
- platform names must be one of `android`, `ios`, `web`, `linux`, `macos`, or `windows`;
- invalid selectors remain visible in normalized output and carry explicit diagnostics rather than being silently dropped.

These stages are transitional and will be removed after the marked-event YAML adapter provides the same behavior.

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
- the compatibility `assets` projection with one path and span per declaration;
- primary `asset_configurations` with paths, optional opaque `flavors`, validated `platforms`, and ordered transformer packages with scalar `args`;
- Flutter font families, asset paths, optional styles, and validated weights from 100 through 900.

Scalar assets and `path: ...` mappings populate both asset representations. Existing consumers can continue reading `assets`; consumers that need selectors or transformations should read `asset_configurations`. The new field uses a Serde default and is omitted from serialized output when empty, so older configuration payloads remain readable.

The official Flutter pubspec localization switch is `flutter.generate`, which is already normalized as `generate_localizations`. Options such as ARB locations, generated class names, and untranslated-message output belong to a future explicit `l10n.yaml` input under `DS-FLUTTER-003`, not to this pubspec model.

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

The additive `configuration` field changes new `pubspec` and `analyze-project` JSON output. Older JSON remains readable because the field has a Serde default. The additive `asset_configurations` field is independently defaulted and keeps the path-only `assets` field intact during the pre-1.0 transition.

## Explicit Limitations

The current production implementation is still a conservative indentation-aware parser, not a complete YAML implementation. Aliases and merge keys remain unsupported by policy. Flow-style environment and top-level Flutter configuration mappings are not supported yet. Asset mappings must currently begin with `path`. Duplicate keys are diagnosed, but invalid input may still retain both conflicting normalized entries until the marked-event adapter owns mapping construction. Selector diagnostics currently point to the containing asset declaration because individual selector-item spans are not yet retained.

## Verification State

The implementation and regression-test changes are present, but this task remains
`in_progress`, not `verified`. No local Rust 1.95.0 toolchain is available in the current
environment, and a successful hosted quality/test/edition run has not been observed for
this head.
