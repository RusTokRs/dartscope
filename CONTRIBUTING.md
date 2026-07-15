# Contributing To DartScope

DartScope accepts focused changes backed by reduced Dart or Flutter examples and a
clear behavioral source.

## Toolchain

The repository requires Rust 1.95. The exact Rust 1.95.0 toolchain, including `rustfmt`
and Clippy, is pinned in `rust-toolchain.toml`; workspace packages inherit
`rust-version = "1.95"` and `edition = "2024"` from the root `Cargo.toml`.

The hosted CI has a dedicated Rust 2024 edition matrix covering Linux and Windows for
the complete workspace, the umbrella crate without default features, and the umbrella
crate with all features. See `docs/development/rust-2024-edition.md`.

## Before A Change

- Read `AGENTS.md` and the library plan.
- Follow `docs/development/rust-code-standards.md` for naming, module ownership,
  refactoring, public APIs, errors, documentation, and tests.
- Search existing fixtures and tests before adding a new extraction rule.
- Decide whether the behavior is normative Dart/Flutter behavior, observed tool
  behavior, an ecosystem convention, or a DartScope heuristic.
- Keep Athanor and Rustok-specific mapping outside this repository.

## Tests And Fixtures

Use an inline unit test for a small parser or resolver edge case. Use a fixture when the
behavior spans multiple files, a pubspec, package configuration, parts, exports, or a
project-level convention.

Each new supported construct should test:

- the expected finding and its kind;
- normalized path and exact byte span;
- confidence or diagnostic behavior when the result is heuristic;
- a nearby negative case that must not produce the finding;
- deterministic output when project input order changes, where applicable.

Real applications are calibration inputs only. Reduce reusable behavior into synthetic
fixtures and do not commit private source trees or generated build output.

## Public API And JSON

DartScope is pre-1.0, but public Rust types and serialized fields are still treated as
compatibility-sensitive. Describe intentional shape changes in the development plan,
add or update a serialization fixture, and avoid claiming schema stability until a
versioned envelope exists.

## Verification

Run:

```powershell
Select-String -Path Cargo.toml -SimpleMatch 'edition = "2024"'
cargo check --workspace --all-targets --locked
cargo check -p dartscope --no-default-features --locked
cargo check -p dartscope --all-features --locked
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The hosted CI repeats these checks on Linux and Windows using the pinned Rust 1.95.0
toolchain and the dedicated edition 2024 matrix.
