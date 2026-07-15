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
- Aliases are emitted as an explicit `Event::Alias`, so DartScope can continue rejecting
  anchors, aliases, and merge keys with path-attributed diagnostics instead of silently
  resolving them.
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
3. reject anchors, aliases, and merge keys with stable diagnostic codes;
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

## Current Migration Status

Completed prerequisites and configuration slices:

- [x] Both public APIs pass through one private `PreparedPubspecSource` boundary.
- [x] Document-count, duplicate-key, malformed-flow, alias-policy, and byte-evidence
  expectations have stable diagnostic codes and regression coverage.
- [x] `tests/pubspec_backend_parity.rs` compares focused and complete configuration,
  shared YAML diagnostics, and CRLF/non-ASCII byte evidence on representative sources.
- [x] The conservative YAML-subset primitives are isolated from the core-owned public
  model and can be deleted after backend cutover.
- [x] `yaml-rust2 = "=0.11.0"` is declared with default features disabled and the
  Cargo-resolved registry graph is recorded in `Cargo.lock`.
- [x] A private `MarkedEventReceiver` bridge builds a marked scalar/sequence/mapping tree,
  rejects aliases and merge keys, detects duplicate keys and additional documents, and
  preserves UTF-8 byte offsets and one-based columns across LF, CRLF, and non-ASCII input.
- [x] A private marked-tree converter maps environment constraints, Flutter booleans,
  assets, selectors, ordered transformers, and fonts into the existing core-owned types.
- [x] A dual-backend configuration parity gate compares conservative and marked-event
  output for structured configuration, duplicate keys, additional documents, invalid
  selectors, CRLF, and non-ASCII evidence.

Remaining before backend cutover:

- [ ] Convert package name and dependency sections/sources from the marked tree into the
  existing core-owned dependency model.
- [ ] Extend dual-backend parity to dependency output and the complete dependency fixture
  suite before changing the public default.
- [ ] Pass the complete Rust 1.95.0 Linux and Windows verification matrix.
- [ ] Remove the conservative parser only after the marked backend is the verified default.

The public APIs still use the conservative backend. The marked implementation remains a
private migration target and cannot leak `yaml-rust2` types into the public contract.

## Migration Sequence

1. Add `yaml-rust2 = "=0.11.0"` and its Cargo-resolved `Cargo.lock` graph. Completed.
2. Add a private marked-event adapter and tests for marker byte offsets on LF, CRLF, and
   non-ASCII input. Completed.
3. Convert environment and Flutter configuration into existing domain models and require
   dual-backend parity. Completed.
4. Convert package name and dependency sections/sources into the existing dependency model.
5. Run the complete dependency/configuration fixtures through both implementations and
   require identical normalized output.
6. Switch `parse_pubspec` to the marked-event adapter while retaining the public model,
   diagnostic paths, and compatibility fields.
7. Remove the line-oriented dependency, configuration, and syntax parsers only after the
   complete fixture suite passes on Linux and Windows.

## Verification Gate

The decision is accepted, but dependency compatibility and backend cutover are not
considered fully verified until these commands run successfully with the repository-pinned
Rust 1.95.0 toolchain:

```powershell
rustc --version
cargo update -p yaml-rust2 --precise 0.11.0
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The marked bridge and configuration converter have additionally passed isolated
compilation, nine unit tests, formatting, and Clippy on Rust 1.85.0. That local
compatibility check is useful evidence but does not replace the required Rust 1.95.0
Linux/Windows matrix or a complete workspace test run.

## Primary Sources

- [`yaml-rust2` 0.11.0 documentation](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/)
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html)
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html)
- [`serde_yaml` deprecation notice](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [`serde_yml` deprecation and migration notice](https://docs.rs/serde_yml/latest/serde_yml/)
- [`serde-saphyr` 0.0.29 documentation](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/)
