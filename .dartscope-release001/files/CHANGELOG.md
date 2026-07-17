# Changelog

All notable changes to DartScope are documented in this file.

The format follows Keep a Changelog, and the project uses semantic versioning while remaining
pre-1.0.

## [Unreleased]

## [0.1.0] - 2026-07-17

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

### Compatibility

- Minimum supported Rust version: 1.95.
- Workspace edition: Rust 2024 with resolver 3.
- Dart and Flutter support is capability-based and source-only; DartScope does not execute SDK
  tools during normal analysis.
- Existing command-facing JSON contracts remain at schema version v1.

[Unreleased]: https://github.com/RusTokRs/dartscope/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/RusTokRs/dartscope/releases/tag/v0.1.0
