# Changelog

All notable changes to DartScope are documented in this file.

The format follows Keep a Changelog, and the project uses semantic versioning while remaining
pre-1.0.

## [Unreleased]

### Added

- Nine publishable Rust crates covering normalized Dart analysis, parsing, package and URI
  resolution, project indexing, optional lint rules, Flutter conventions, versioned JSON contracts,
  a thin umbrella API, and the `dartscope` CLI.
- Conservative source-only Dart and Flutter analysis with exact spans, diagnostics, capability
  metadata, namespace and reference resolution, GraphQL contract linking, and package-aware Flutter
  catalogs.
- Stable v1 CLI JSON envelopes, deterministic fixtures, explicit exit codes, and Linux/Windows
  process-level coverage.
- Versioned opt-in ecosystem conventions for `go_router`, Provider, Riverpod, and BLoC.
- Release metadata, package-order validation, package archives, support documentation, and a
  manually gated crates.io publishing workflow.
- The audited `0.2` development queue, beginning with immutable SHA-pinned Node 24 Actions,
  `actionlint`, enforceable workflow permissions, and read-only pull-request execution.
- `dartscope lint` with explicit versioned TOML configuration, a `dartscope.lint-analysis` v1 JSON
  contract, SARIF 2.1.0 output, deterministic thresholds, and stable process exit codes.
- A stateful workspace index foundation with normalized file/configuration mutations, immutable shared
  snapshots, deterministic reverse invalidation evidence, per-source URI/reference caches, and operation
  counters.

### Compatibility

- Minimum supported Rust version: 1.95.
- Workspace edition: Rust 2024 with resolver 3.
- Dart and Flutter support is capability-based and source-only; DartScope does not execute SDK
  tools during normal analysis.
- Existing command-facing JSON contracts remain at schema version v1.

Release notes remain under `Unreleased` until the exact version tag exists. The release process moves
this content to a dated version section and adds compare/release links in the same release operation.

[Unreleased]: https://github.com/RusTokRs/dartscope/commits/main
