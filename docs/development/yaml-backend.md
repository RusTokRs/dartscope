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

The first dependency declaration should pin `=0.11.0` with default features disabled:

```toml
yaml-rust2 = { version = "=0.11.0", default-features = false }
```

The dependency and lockfile must be added together in a commit that passes the complete
Rust 1.95.0 verification suite. Parser types must remain private and must not appear in
`dartscope-core` or public APIs.

## Why This Backend

- `yaml-rust2` 0.11.0 is a maintained pure-Rust YAML 1.2 implementation.
- Its documented MSRV is Rust 1.65.0 with default features disabled, below DartScope's
  pinned Rust 1.95.0 toolchain.
- The low-level parser exposes `Event`, `MarkedEventReceiver`, and byte-indexed `Marker`
  values. This lets the adapter preserve dependency-key and environment-key spans rather
  than reconstructing them from normalized values.
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
5. convert byte markers and verified line/column coordinates into `SourceSpan`;
6. preserve dependency order only as input evidence while keeping public output ordering
   deterministic where the existing API requires it;
7. normalize package name, dependency sections, environment constraints, Flutter assets,
   fonts, and generation configuration into existing core-owned types;
8. keep `version_or_source` only as the documented pre-1.0 compatibility field;
9. avoid filesystem access, includes, implicit command execution, or network access;
10. emit no `yaml-rust2` type from a public function or public struct.

## Migration Sequence

1. Add `yaml-rust2 = "=0.11.0"` and regenerate `Cargo.lock` on Rust 1.95.0.
2. Add a private marked-event adapter and tests for marker byte offsets on LF, CRLF, and
   non-ASCII input.
3. Run the current dependency/configuration fixtures through both implementations and
   require identical normalized output.
4. Add negative fixtures for duplicate keys, aliases, merge keys, multiple documents,
   malformed block syntax, and malformed flow syntax.
5. Switch `parse_pubspec` to the marked-event adapter while retaining the public model,
   diagnostic paths, and compatibility fields.
6. Remove the line-oriented dependency, configuration, and syntax parsers only after the
   complete fixture suite passes on Linux and Windows.

## Verification Gate

The decision is accepted, but dependency compatibility is not considered verified until
these commands run successfully with the repository-pinned Rust 1.95.0 toolchain:

```powershell
rustc --version
cargo update -p yaml-rust2 --precise 0.11.0
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The dependency must not be committed without its generated lockfile update.

## Primary Sources

- [`yaml-rust2` 0.11.0 documentation](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/)
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html)
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html)
- [`serde_yaml` deprecation notice](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [`serde_yml` deprecation and migration notice](https://docs.rs/serde_yml/latest/serde_yml/)
- [`serde-saphyr` 0.0.29 documentation](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/)
