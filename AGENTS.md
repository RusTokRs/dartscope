# DartScope Agent Guide

This file is the entrypoint for agents changing DartScope.

## Required Reading

Read these files before implementation:

1. `README.md`
2. `docs/development/dartscope-library-plan.md`
3. `docs/development/ds-index-006-progress-2026-07-21.md`
4. `docs/development/rust-code-standards.md`
5. `docs/development/rust-toolchain.md`
6. `docs/reference-strategy.md`
7. `CONTRIBUTING.md`

Then read the source and tests for every crate you intend to modify.

The Rust code standard is mandatory. Its naming, ownership, refactor-trigger, public
API, error, documentation, and testing rules apply to every code change.

## Required Toolchain

Use the repository-pinned Rust 1.95.0 toolchain. `rust-toolchain.toml` supplies Cargo,
rustfmt, Clippy, and rustdoc; all workspace crates inherit `rust-version = "1.95"` and
`edition = "2024"` from the root manifest. The virtual workspace explicitly uses Cargo
resolver 3. Do not introduce a second Rust version, resolver, edition, or an unpinned CI
toolchain.

The edition migration contract lives in `docs/development/rust-2024-edition.md`.

## Repository Boundary

- DartScope is the standalone Rust toolkit at `D:\DartScope`.
- It must not depend on Athanor or emit Athanor domain objects as its primary API.
- Athanor integration belongs in `D:\Athanor` and consumes DartScope through an adapter.
- Rustok is a calibration project, not a source of general Dart or Flutter semantics.
- Do not copy private or large real-project sources into this repository. Reduce a case
  to a small synthetic fixture.

## Source Of Truth

- Use official Dart and Flutter specifications and documentation for language and
  framework behavior.
- Label ecosystem conventions and local heuristics explicitly.
- Do not broaden a parser heuristic from memory alone. Record its source class in the
  test name, test comment, or `docs/reference-strategy.md`.
- Preserve uncertainty through confidence metadata or diagnostics.

## Task Workflow

1. Select the first unblocked task from the ordered queue in the library plan.
2. Reproduce the missing or incorrect behavior with a focused test or fixture.
3. Make the smallest change that fixes that case without adding consumer-specific logic.
4. Update public documentation and roadmap status in the same change.
5. Run the required verification commands.
6. Report changed files, commands run, and remaining limitations.

Do not mark a task complete when only the happy path is tested. Every completed task
must satisfy its acceptance criteria and definition of done in the plan.

## Required Verification

Run from `D:\DartScope`:

```powershell
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

For CLI changes, also run the affected command against a repository fixture or a small
temporary project. For feature changes, check the relevant umbrella feature combination.

When a touched function or module is near a refactor trigger, run the selected
maintainability audit from `docs/development/rust-code-standards.md` for that crate. Do
not suppress a complexity warning merely to finish the feature.

## Change Safety

- Treat `dartscope-core` and serialized public structs as compatibility-sensitive.
- Do not remove or rename a serialized field without a migration note and schema test.
- Keep `dartscope-index` independent from parser internals.
- Keep `dartscope-flutter` optional for pure Dart consumers.
- Do not add filesystem or process I/O to core analysis crates without an explicit port.
- Preserve unrelated working-tree changes.

## Current Next Step

Continue `DS-INDEX-006` with direct property and operator targets. Add parser-produced facts for
getter and field reads, setter and field writes, and operator invocations while keeping exact owner
evidence and static-versus-instance mode explicit. Preserve privacy, validated parts, ambiguity,
conditional imports, not-visible outcomes, external-unindexed URIs, and snapshot parity. Keep
arbitrary receiver inference, inheritance, extension selection, dynamic dispatch, patterns, and
flow-sensitive behavior behind later focused slices.
