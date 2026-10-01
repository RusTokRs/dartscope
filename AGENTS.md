# DartScope Agent Guide

This file is the entrypoint for agents changing DartScope.

## Required Reading

Read these files before implementation:

1. `README.md`
2. `docs/development/dartscope-library-plan.md`
3. `docs/development/ds-index-006-progress-2026-07-22.md`
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

For a change to the parser, the declaration inventory or the incremental index, also run the mutation
hunt described in `docs/development/fuzzing.md` (`DARTSCOPE_MUTATION_ROUNDS`, release build): the
end-to-end mutation tests in `cargo test` use small defaults.

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

Unqualified same-owner member navigation is implemented and verified: parser-side
`unqualified_member_references` emits invocation/read/write facts only when the enclosing callable
supplies one exact owner symbol ID, the owner directly declares a matching method, field, getter, or
setter, and no visible lexical binding, parameter, local function, or enclosing-owner member shadows
the spelling. Continue `DS-INDEX-006` with the next bounded, evidence-gated slice: inherited-member or
extension selection for an exact owner type, or local-function lexical bindings. Each slice must arrive
with nearby-shadowing fixtures, exact spans, an explicit compatibility note, and full-build versus
immutable-snapshot parity. Keep arbitrary receiver inference, cascades, null-aware access, dynamic
dispatch, patterns, and flow-sensitive behavior behind later focused slices.

The 2026-09-30 engineering audit (`docs/development/audit-findings-2026-09-30.md`) was worked off on the
audit branch; section 16 of that report is the status of every finding. The reference passes of
`dartscope-parse` (`lexical_reads`, `lexical_writes`, `identifier_references`, `member_references`,
`property_references`, `operator_references`, `lexical_regions`, `lexical_bindings`) are linear in the size
of one file because they take every per-token answer from structures built once per file: `FileFacts`
(`file_facts.rs`) with `DeclarationTables` (`declaration_tables.rs`) and `SourceStructure`
(`source_structure.rs`), plus `BindingIndex` (`binding_index.rs`) per pass, all on the interval primitives
of `interval_index.rs`. Keep it that way: a pass must not walk `analysis.declarations`, the bindings or the
references found so far, or rescan the text around a token, from inside a per-token loop. When a pass needs
a new question answered, add it to one of those structures together with a test that compares it with the
scan it replaces (the `linear` modules next to the existing ones), and check a restructuring with the
differential run described in `docs/development/fuzzing.md`; do not assert wall-clock time.

A single file analysis is also bounded in memory and in the work of its unbounded scans: invocation facts copy
source text under a `CopyBudget`, and the declaration inventory charges its header, end and body scans to
`Scans` (32 times the file plus 1 MiB each, with the warnings `invocation_facts_truncated` and
`declaration_scan_truncated` when a budget ends the analysis early; `docs/development/json-contracts.md`,
"Analysis budgets"). A new scan over source text must charge its work to one of them, and a new hostile shape
belongs in the sweep of `crates/dartscope-parse/tests/adversarial_shapes.rs` (run by hand; the commands are in
`docs/development/cli-input-limits.md`).

The code keeps the layout that `docs/development/rust-code-standards.md` asks for: `lib.rs` files only declare
modules and re-export (`dartscope-core` by domain, `dartscope-index` with `incremental/` split by concern),
a function that needs more than a handful of inputs takes a context struct (`TypeScan`, `ParameterScan`,
`RebuildTrigger`) instead of `allow(clippy::too_many_arguments)`, and there is no `allow(dead_code)`. Keep it
that way: delete code that is not used and bundle the inputs that stay fixed during a scan.

The URI handling of `dartscope-resolve` is its own module (`uri.rs`), not a dependency, because it decides which
paths a project may name. Change it only together with a test from RFC 3986 section 5.4 or from the cases in
its module, and keep the `uri_normalization` fuzz target meaningful: a `project_path` is a plain relative path or
`None`.

What the 2026-09-30 audit left open is listed in section 16.5 of its report.

Separately, the 2026-09-25 review consolidated Dart identifier scanning into
`crates/dartscope-parse/src/identifiers.rs` after finding that thirteen local character classes had
truncated every name containing `$`. The same consolidation for numeric literals, string escapes, and
metadata handling is the highest-value next architectural slice; details and evidence are in
`docs/development/audit-findings-2026-09-25.md`.
