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

Completed migration and cutover slices:

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
- [x] A private backend selector runs the same complete and focused pubspec contracts through
  conservative and marked implementations.
- [x] Dual-backend parity covers package names, dependency order and sections, typed and
  compatibility source representations, environment and Flutter configuration, shared
  diagnostics, malformed input recovery, CRLF, duplicate keys, and non-ASCII byte evidence.
- [x] The complete Rust 1.95.0 Linux and Windows matrix passed before cutover.
- [x] `parse_pubspec` and `parse_pubspec_configuration` now use the marked backend by default.
- [x] The marked-default workspace passes the complete local Linux Definition Of Done on the
  repository-pinned Rust 1.95.0 toolchain.

Remaining cleanup:

- [ ] Confirm the hosted Rust 1.95.0 Linux and Windows matrix on the marked-default commit.
- [ ] Remove the conservative dependency/configuration implementation only after that
  cutover commit is green, while retaining characterization evidence where it remains useful.
- [ ] Add remaining localization-owned fields and define a versioned policy for validating
  Flutter flavor and platform names.

`yaml-rust2` types remain private to `dartscope-parse`. The conservative implementation is
retained only as a private parity oracle during the verified cutover window; it is no longer
the public default.

## Migration Sequence

1. Add `yaml-rust2 = "=0.11.0"` and its Cargo-resolved `Cargo.lock` graph. Completed.
2. Add a private marked-event adapter and UTF-8 marker tests. Completed.
3. Convert environment and Flutter configuration and require dual-backend parity. Completed.
4. Convert package names and dependency sections/sources. Completed.
5. Run dependency/configuration contracts through both implementations. Completed.
6. Pass the repository-pinned Rust 1.95.0 Linux and Windows matrix before cutover. Completed.
7. Introduce one private backend selector and switch both public pubspec APIs to marked-event
   parsing without changing the public model or compatibility fields. Completed.
8. Pass the hosted matrix on the marked-default commit. Pending.
9. Remove the line-oriented dependency/configuration implementation after the marked-default
   commit is verified cross-platform. Pending.

## Verification Gate

The marked-default tree passes these commands locally on the repository-pinned Rust 1.95.0
toolchain:

```powershell
rustc --version
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The pre-cutover commit `4d1380ccdcd634f3200e48b7f2af88a7bbef203a` also received a
successful `dartscope/ci` status for the Rust 1.95.0 Linux/Windows matrix. The next required
evidence is the same hosted matrix on the marked-default cutover commit.

## Primary Sources

- [`yaml-rust2` 0.11.0 documentation](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/)
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html)
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html)
- [`serde_yaml` deprecation notice](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [`serde_yml` deprecation and migration notice](https://docs.rs/serde_yml/latest/serde_yml/)
- [`serde-saphyr` 0.0.29 documentation](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/)
