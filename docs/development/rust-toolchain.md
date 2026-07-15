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
- Hosted checks: Linux quality gates and Linux/Windows tests install Rust 1.95.0.

All eight crate manifests use `rust-version.workspace = true` and
`edition.workspace = true`. A crate must not declare a second local Rust version.

## Edition

The workspace remains on Rust edition 2021. Compiler version/MSRV and language edition
are independent settings. A future edition 2024 migration requires a separate task with
fixture, Clippy, documentation, public API, and downstream compatibility checks; it must
not be bundled into a toolchain pin change.

## Sources Of Truth

| Concern | Source |
| --- | --- |
| minimum supported compiler | root `Cargo.toml` |
| exact local/CI toolchain and components | `rust-toolchain.toml` |
| hosted commands and operating systems | `.github/workflows/ci.yml` |
| contributor commands | `CONTRIBUTING.md` |
| agent commands and constraints | `AGENTS.md` |
| release/task acceptance | `docs/development/dartscope-library-plan.md` |

Documentation may repeat the version for clarity but may not define a different one.

## Prohibited Overrides

Do not introduce another Rust channel or version in:

- individual crate manifests;
- `.cargo/config.toml` or target-specific rustflags used to simulate compatibility;
- `.tool-versions`, `mise.toml`, or devcontainer settings;
- Dockerfiles, task runners, release automation, or additional workflows;
- dependency evaluation notes or release matrices.

If one of these files is added later, it must reference Rust 1.95.0 or rely directly on
`rust-toolchain.toml`.

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
cargo fmt --all -- --check
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
```

The first four commands must report tools from Rust 1.95.0. A change that modifies
`Cargo.toml` dependencies must regenerate `Cargo.lock` with the pinned toolchain before
it is committed.
