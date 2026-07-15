---
id: doc://docs/development/rust-2024-edition.md
kind: migration_note
language: en
status: active
---

# Rust 2024 Edition Migration

Migration date: 2026-07-15.

## Scope

The complete DartScope workspace now declares Rust edition 2024 through the root
`Cargo.toml`. All eight crates inherit the edition with `edition.workspace = true`.
The compiler policy remains Rust 1.95 with the exact Rust 1.95.0 toolchain pinned in
`rust-toolchain.toml`.

This is a workspace-wide source-compatibility migration. It does not change public JSON
fields, crate features, package versions, or the Rust MSRV.

## Dedicated CI Matrix

Edition checks run in a separate `edition-2024` job rather than being hidden inside the
normal test job. The matrix contains six checks:

| Operating system | Check |
| --- | --- |
| Linux | workspace and all targets |
| Windows | workspace and all targets |
| Linux | umbrella crate without default features |
| Windows | umbrella crate without default features |
| Linux | umbrella crate with all features |
| Windows | umbrella crate with all features |

The normal quality job still owns rustfmt, Clippy with warnings denied, and rustdoc. The
normal Linux/Windows test matrix still owns the complete workspace test suite.

## Edition-Specific Review

Before the migration is considered verified, review compiler output for the Rust 2024
changes that can require source edits, including:

- newly unsafe standard-library APIs such as environment mutation;
- unsafe attributes and extern blocks;
- reserved keywords such as `gen`;
- match ergonomics and temporary lifetime changes;
- macro fragment-specifier changes;
- prelude additions that can create method-resolution ambiguity.

A repository code search did not identify explicit uses of `std::env::set_var`,
`std::env::remove_var`, `#[no_mangle]`, or an identifier named `gen` before the manifest
switch. This search is only a preliminary audit; compiler and Clippy results remain the
source of truth.

## Required Local Validation

Run with the repository-pinned Rust 1.95.0 toolchain:

```powershell
rustc --version
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

Because the workspace manifest is already on edition 2024, `cargo fix --edition` is not
used as an unattended CI mutation step. Any compiler-suggested edition rewrites must be
applied deliberately, reviewed, and committed as normal source changes.

## Compatibility Boundary

Downstream users compile published DartScope crates using the edition declared by each
crate package. Rust editions are interoperable across dependency boundaries, so a
consumer does not need to migrate its own crate to edition 2024 merely to depend on
DartScope. The consumer must still satisfy DartScope's Rust 1.95 MSRV.

## Verification State

The manifest and dedicated matrix are implemented. The migration remains unverified
until the hosted Rust 1.95.0 checks complete successfully on Linux and Windows.
