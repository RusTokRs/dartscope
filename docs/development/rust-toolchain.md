---
id: doc://docs/development/rust-toolchain.md
kind: toolchain_policy
language: en
status: active
---

# Rust Toolchain Policy

DartScope uses one Rust toolchain policy across local development, all workspace crates,
CI, documentation, parser dependency decisions, and release planning.

## Required Version

- Workspace MSRV: Rust 1.95, declared once as `rust-version = "1.95"` in the root
  `Cargo.toml`.
- Exact repository toolchain: Rust 1.95.0, pinned in `rust-toolchain.toml`.
- Required components: Cargo, rustc, rustfmt, Clippy, and rustdoc from Rust 1.95.0.
- Hosted checks: Linux quality gates, Linux/Windows tests, and the dedicated edition matrix
  install Rust 1.95.0.

All eight crate manifests use `rust-version.workspace = true` and
`edition.workspace = true`. A crate must not declare a second local Rust version or
edition.

## Edition And Resolver

The complete workspace uses Rust edition 2024, declared once as `edition = "2024"` in
the root `Cargo.toml`. Every crate inherits the setting through
`edition.workspace = true`.

DartScope is a virtual workspace, so it explicitly declares `resolver = "3"` in the
workspace table. Resolver 3 enables Rust-version-aware dependency selection and must not
be downgraded independently from the edition policy.

Edition compatibility is verified in a dedicated CI matrix separate from the normal
quality and test jobs. The matrix covers Linux and Windows for:

- the complete workspace with all targets;
- the umbrella crate with no default features;
- the umbrella crate with all features.

The migration contract and edition-specific risks are recorded in
`docs/development/rust-2024-edition.md`.

## Sources Of Truth

| Concern | Source |
| --- | --- |
| minimum supported compiler, resolver, and workspace edition | root `Cargo.toml` |
| exact local/CI toolchain and components | `rust-toolchain.toml` |
| hosted commands, operating systems, and edition matrix | `.github/workflows/ci.yml` |
| contributor commands | `CONTRIBUTING.md` |
| agent commands and constraints | `AGENTS.md` |
| release/task acceptance | `docs/development/dartscope-library-plan.md` |

Documentation may repeat the version, resolver, or edition for clarity but may not
define a different one.

## Prohibited Overrides

Do not introduce another Rust channel, version, resolver, or edition in:

- individual crate manifests;
- `.cargo/config.toml` or target-specific rustflags used to simulate compatibility;
- `.tool-versions`, `mise.toml`, or devcontainer settings;
- Dockerfiles, task runners, release automation, or additional workflows;
- dependency evaluation notes or release matrices.

If one of these files is added later, it must reference Rust 1.95.0, resolver 3, and
edition 2024 or rely directly on the root workspace settings.

At the time this policy was created, the repository had no `.cargo/config.toml`,
Dependabot toolchain override, devcontainer, Dockerfile, release-plz configuration,
Justfile, rustfmt configuration, Clippy configuration, or `.tool-versions` file.

## Verification

Run from the repository root:

```powershell
rustc --version
cargo --version
rustfmt --version
cargo clippy --version
Select-String -Path Cargo.toml -SimpleMatch 'resolver = "3"'
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

The first four commands must report tools from Rust 1.95.0. A change that modifies
`Cargo.toml` dependencies must regenerate `Cargo.lock` with the pinned toolchain before
it is committed.
