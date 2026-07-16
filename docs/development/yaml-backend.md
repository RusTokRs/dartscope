---
id: doc://docs/development/yaml-backend.md
kind: architecture_decision
language: en
status: accepted
---

# Pubspec YAML Backend Decision

Decision date: 2026-07-15.

## Decision

DartScope will migrate structured pubspec parsing to `yaml-rust2` 0.11.x behind a
private adapter in `dartscope-parse`.

The dependency is pinned with default features disabled:

```toml
yaml-rust2 = { version = "=0.11.0", default-features = false }
```

The dependency and lockfile must remain in the same verified repository state. Parser
types must remain private and must not appear in `dartscope-core` or public APIs.

## Why This Backend

- `yaml-rust2` 0.11.0 is a maintained pure-Rust YAML 1.2 implementation.
- Its documented MSRV is Rust 1.65.0 with default features disabled, below DartScope's
  pinned Rust 1.95.0 toolchain.
- The low-level parser exposes `Event`, `MarkedEventReceiver`, and `Marker` values with
  character indices plus line and column coordinates. The private adapter precomputes a
  UTF-8 character-to-byte table before converting markers to `SourceSpan`; this preserves
  byte evidence for LF, CRLF, and non-ASCII sources without rescanning the input per event.
- Aliases are emitted as an explicit `Event::Alias`, so DartScope can diagnose anchors and
  reject alias or merge-key resolution with path-attributed diagnostics instead of silently
  expanding YAML references.
- The parser works from in-memory input and does not require filesystem or process I/O.

DartScope will use the low-level marked event API, not `YamlLoader`, because the public
model requires exact evidence spans and explicit unsupported-syntax diagnostics.

## Alternatives Reviewed

### `serde_yaml`

Rejected. The latest release is explicitly deprecated and the project states that it is
no longer maintained.

### `serde_yml`

Rejected. Version 0.0.13 is an explicitly deprecated compatibility shim and directs
users to maintained alternatives.

### `serde_yaml_ng`

Not selected. It provides a familiar Serde value model but documents YAML 1.1 support
and depends on `unsafe-libyaml`. It does not offer a better fit for DartScope's marked,
event-oriented evidence requirements.

### `serde-saphyr`

Viable secondary candidate, but not selected for the first adapter. Version 0.0.29 is
actively developed, supports typed deserialization, byte spans for string inputs,
duplicate-key policy, and merge-key policy. Its strongly typed design is less direct for
dynamic dependency names and explicit event-level alias rejection, and its published
metadata does not declare an MSRV. Re-evaluate only if the event adapter becomes
unnecessarily complex.

### `saphyr`

Viable marked-DOM alternative, but `yaml-rust2` has the simpler compatibility argument:
an explicit low MSRV and a documented event stream that distinguishes aliases.

## Adapter Contract

The private adapter must:

1. accept only the in-memory UTF-8 source from `PubspecInput`;
2. reject streams containing more than one YAML document;
3. diagnose anchors and reject alias or merge-key values without resolving references;
4. reject duplicate mapping keys rather than choosing first-wins or last-wins behavior;
5. convert marked character indices and verified line/column coordinates into byte-based
   `SourceSpan` values;
6. preserve dependency order only as input evidence while keeping public output ordering
   deterministic where the existing API requires it;
7. normalize package name, dependency sections, environment constraints, Flutter assets,
   fonts, and generation configuration into existing core-owned types;
8. keep `version_or_source` only as the documented pre-1.0 compatibility field;
9. avoid filesystem access, includes, implicit command execution, or network access;
10. emit no `yaml-rust2` type from a public function or public struct.

An anchored mapping may still be interpreted as its literal local value after the stable
unsupported-alias diagnostic is emitted. DartScope never resolves another node through that
anchor. A dependency whose value is an alias or merge mapping is omitted from normalized
output because its effective value cannot be established without reference expansion.

## Current Migration Status

The parser migration and cleanup are complete:

- [x] Both public APIs pass through one private `PreparedPubspecSource` boundary.
- [x] Document-count, duplicate-key, malformed-flow, indentation, alias-policy, and
  byte-evidence expectations have stable diagnostic codes and regression coverage.
- [x] `yaml-rust2 = "=0.11.0"` is declared with default features disabled and the
  Cargo-resolved registry graph is recorded in `Cargo.lock`.
- [x] A private `MarkedEventReceiver` bridge builds a marked scalar/sequence/mapping tree,
  diagnoses anchors, rejects alias and merge-key values, detects duplicate keys and
  additional documents, and preserves UTF-8 byte offsets and one-based columns across LF,
  CRLF, and non-ASCII input.
- [x] Marked-tree converters map package names, dependency sections and sources, environment
  constraints, Flutter booleans, assets, selectors, ordered transformers, and fonts into the
  existing core-owned types.
- [x] Bare wildcard constraints are sanitized with a one-byte replacement before marked
  parsing and restored from syntax evidence without changing source byte offsets.
- [x] Pre-cutover dual-backend parity covered package names, dependency order and sections,
  typed and compatibility source representations, environment and Flutter configuration,
  shared diagnostics, malformed input recovery, CRLF, duplicate keys, and non-ASCII byte
  evidence.
- [x] `parse_pubspec` and `parse_pubspec_configuration` use the marked backend by default.
- [x] The marked-default tree passed the complete local Linux Definition Of Done and the
  hosted Rust 1.95.0 Linux/Windows matrix. Commit `566edbb0da58799d227a4615713631aefaf25978`
  received successful quality, Linux/Windows tests, all six edition/feature checks, and the
  aggregate `dartscope/ci` status.
- [x] The conservative dependency, configuration, and structured-asset runtime modules were
  removed after cross-platform verification. Their representative source matrix remains as
  explicit marked-backend contract tests.

The final `DS-PUB-002` model slice adds `flutter.default-flavor` and the public
`PubspecFlutterAssetSelectorPolicy::V1`. Policy v1 treats non-empty flavor names as opaque
application values and validates platforms against `android`, `ios`, `web`, `linux`, `macos`,
and `windows`. Localization configuration beyond `flutter.generate` remains an explicit
`l10n.yaml` input owned by `DS-FLUTTER-003`, not the pubspec model. Commit `88e65e3c017b58ec9b64907efdeaa0e8d2ee67af` passed the hosted Rust 1.95.0 Linux/Windows matrix for this final model slice.

`yaml-rust2` types remain private to `dartscope-parse`. The small line-evidence scanner is
retained only for stable compatibility diagnostics that require raw indentation and wildcard
information; dependency and configuration values are constructed exclusively from the marked
YAML tree.

## Migration Sequence

1. Add `yaml-rust2 = "=0.11.0"` and its Cargo-resolved `Cargo.lock` graph. Completed.
2. Add a private marked-event adapter and UTF-8 marker tests. Completed.
3. Convert environment and Flutter configuration and require dual-backend parity. Completed.
4. Convert package names and dependency sections/sources. Completed.
5. Run dependency/configuration contracts through both implementations. Completed.
6. Pass the repository-pinned Rust 1.95.0 Linux and Windows matrix before cutover. Completed.
7. Switch both public pubspec APIs to marked-event parsing without changing public models or
   compatibility fields. Completed.
8. Pass the hosted matrix on the marked-default commit. Completed.
9. Remove the conservative dependency/configuration implementation and retain explicit
   marked contract tests. Completed.
10. Normalize `flutter.default-flavor` and expose selector validation policy v1 without
    changing legacy JSON readability. Completed.

## Verification Gate

The marked-only tree passes these commands on the repository-pinned Rust 1.95.0 toolchain:

```powershell
rustc --version
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The hosted verification commit `566edbb0da58799d227a4615713631aefaf25978` published
successful statuses for quality, workspace tests on Linux and Windows, workspace/all-targets
and umbrella minimal/all-features checks on both operating systems, and aggregate
`dartscope/ci`. The selector-policy extension also passed the complete hosted matrix on commit `88e65e3c017b58ec9b64907efdeaa0e8d2ee67af`.

## Primary Sources

- [`yaml-rust2` 0.11.0 documentation](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/)
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html)
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html)
- [`serde_yaml` deprecation notice](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [`serde_yml` deprecation and migration notice](https://docs.rs/serde_yml/latest/serde_yml/)
- [`serde-saphyr` 0.0.29 documentation](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/)
