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

Completed prerequisites and private backend slices:

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
  diagnoses anchors, rejects alias and merge-key values, detects duplicate keys and
  additional documents, and preserves UTF-8 byte offsets and one-based columns across LF,
  CRLF, and non-ASCII input.
- [x] A private marked-tree converter maps environment constraints, Flutter booleans,
  assets, selectors, ordered transformers, and fonts into the existing core-owned types.
- [x] A private dependency converter maps package names, `dependencies`,
  `dev_dependencies`, and `dependency_overrides`, including scalar, SDK, path, git,
  hosted, workspace, version, fallback, block-mapping, and flow-mapping source shapes.
- [x] Bare wildcard constraints are sanitized with a one-byte replacement before marked
  parsing and restored from syntax evidence without changing any source byte offsets.
- [x] Dual-backend parity compares package names, dependency order and sections, typed and
  compatibility source representations, environment and Flutter configuration, shared
  diagnostics, CRLF, duplicate keys, and non-ASCII byte evidence.
- [x] Negative marked-backend tests confirm that alias and merge dependency values do not
  create fabricated dependencies and malformed inline mappings are omitted with
  `pubspec_invalid_yaml`.

Remaining before backend cutover:

- [ ] Pass the complete Rust 1.95.0 Linux and Windows verification matrix.
- [ ] Switch the public pubspec APIs to the marked backend after that matrix is green.
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
   Completed.
5. Run representative dependency/configuration contracts through both implementations and
   require identical normalized output. Completed.
6. Pass the complete repository-pinned Rust 1.95.0 Linux and Windows matrix.
7. Switch `parse_pubspec` and the focused configuration API to the marked-event adapter
   while retaining the public model, diagnostic paths, and compatibility fields.
8. Remove the line-oriented dependency, configuration, and syntax parsers only after the
   complete fixture suite passes on Linux and Windows with the marked backend as default.

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

The marked bridge and configuration converter previously passed isolated compilation,
unit tests, formatting, and Clippy on Rust 1.85.0. The dependency converter, complete
private parity composition, and negative dependency recovery additionally pass an isolated
build with the real `yaml-rust2` 0.11.0 dependency on Rust 1.88.0: six focused tests,
`rustfmt --check`, and Clippy with `-D warnings`. These compatibility checks are useful
evidence but do not replace the required Rust 1.95.0 Linux/Windows matrix or a complete
workspace test run.

## Primary Sources

- [`yaml-rust2` 0.11.0 documentation](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/)
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html)
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html)
- [`serde_yaml` deprecation notice](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [`serde_yml` deprecation and migration notice](https://docs.rs/serde_yml/latest/serde_yml/)
- [`serde-saphyr` 0.0.29 documentation](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/)
