# DartScope

DartScope is a standalone Rust toolkit for Dart and Flutter code intelligence.

It is a community-facing library ecosystem, not an Athanor adapter. Downstream tools can use DartScope for parsing, indexing, JSON export, CI checks, editor tooling, migration workflows, and graph adapters without depending on Athanor or Rustok-specific domain types.

## Current Scope

This repository is in the first bootstrap stage:

- `dartscope-core` owns stable analysis types, spans, diagnostics, and pubspec models.
- `dartscope-parse` provides a conservative file-level MVP for imports, exports, parts, declarations, simple Flutter widget hints, and `pubspec.yaml` dependency discovery.
- `dartscope-json` provides stable JSON serialization helpers.
- `dartscope-cli` exposes a small command-line wrapper for local smoke testing.
- `dartscope` is a thin umbrella crate with feature-gated re-exports.

## Non-Goals

- Do not depend on Athanor crates.
- Do not expose parser-specific AST nodes as the main public API.
- Do not pretend heuristic findings are complete.
- Do not run `dart` or `flutter` commands during normal analysis.

## Quick Start

```powershell
cargo test --workspace
cargo run -p dartscope-cli -- analyze-file path\to\file.dart
cargo run -p dartscope-cli -- pubspec path\to\pubspec.yaml
```

## Reference Strategy

DartScope behavior should be traceable to official Dart and Flutter sources first, with implementation and ecosystem tools used as practical references rather than sources of truth. See [docs/development/dartscope-library-plan.md](docs/development/dartscope-library-plan.md).

