# DartScope

DartScope is a standalone Rust toolkit for Dart and Flutter code intelligence.

It is a community-facing library ecosystem, not an Athanor adapter. Downstream tools can use DartScope for parsing, indexing, JSON export, CI checks, editor tooling, migration workflows, and graph adapters without depending on Athanor or Rustok-specific domain types.

## Current Scope

This repository is in the first bootstrap stage:

- `dartscope-core` owns stable analysis types, spans, diagnostics, and pubspec models.
- `dartscope-parse` provides a conservative file-level MVP for imports, exports, parts, declarations, simple Flutter widget hints, Dart-embedded GraphQL operations and uses, and `pubspec.yaml` dependency discovery.
- `dartscope-index` performs project-level linking over normalized analysis results. Its
  first API resolves GraphQL operation uses conservatively and compares operation,
  client-call, and variable contracts without depending on parser internals.
- `dartscope-resolve` parses official package configuration v2 inputs and owns package
  and URI resolution primitives without performing filesystem I/O.
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
cargo run -p dartscope-cli -- analyze-project path\to\flutter_project
cargo run -p dartscope-cli -- graphql-contracts path\to\flutter_project
cargo run -p dartscope-cli -- uri-graph path\to\flutter_project
cargo run -p dartscope-cli -- uri-graph path\to\flutter_project --env dart.library.io=true
```

`analyze-project` recursively scans `.dart` files and `pubspec.yaml` files, skips
`.git`, `.dart_tool`, `build`, and `target`, and returns a deterministic JSON summary
plus per-file analysis output. Current output includes top-level string constants,
GraphQL operation documents from Dart raw string constants, declared operation
variables, client uses such as `gql(operationConstant)` inside
`query`/`mutate`/`subscribe` calls, supplied client variable names, conservative Flutter
widget hints, and `GoRoute` hints with `resolved_path` when a route path can be resolved
from same-file string constants. Use it as the first real-project feedback loop before
adding broader parser or Flutter convention support.

`graphql-contracts` links a `gql(operationConstant)` use only through Dart visibility:
an unambiguous same-file declaration, direct import, or transitive re-export. Each
binding retains source paths and spans for both ends, exposes its `resolution_basis`,
and reports call-kind compatibility, missing variables, and unexpected variables.
Missing, non-visible, and ambiguous declarations remain explicit unresolved uses.

The library-level `build_uri_graph` API resolves direct relative `import`, `export`,
and `part` URIs against indexed files. It also resolves `package:` URIs for packages
whose `pubspec.yaml` and `lib/` sources are present in the project analysis. SDK
`dart:` URIs remain external, while packages absent from the loaded source set are
reported as `unindexed_package`; DartScope does not claim they are missing without an
official package configuration.

`analyze_part_links` verifies resolved `part` targets against their reverse `part of`
directive. It distinguishes a missing indexed file, an unresolved external target, a
missing `part of`, and a reference to a different library, retaining evidence spans for
both directives when available.

Import and export analysis preserves namespace combinators (`show` and `hide`), import
prefixes, and `deferred`. GraphQL contract linking can use an unprefixed direct import
when its resolved target contains the operation and its combinators expose the name;
the binding is marked `direct_import`. Transitive exports are followed with cycle
protection and produce `re_export`; private names beginning with `_` are not exposed.
Conditional import/export directives preserve the default URI and every
`if (condition) URI` alternative, including multi-line source spans. The URI graph
resolves every branch without selecting a platform by default. Callers that know the
Dart compilation environment can pass `DartIndexOptions` with a
`DartCompilationEnvironment`; DartScope then selects the first matching configured URI
in source order, using `"true"` for conditions without an explicit `==` comparison.
Symbol resolution returns `conditional_environment_required` only when a conditional
namespace must be followed and no environment was provided.
The CLI accepts repeated `--env key=value` pairs for `uri-graph` and
`graphql-contracts` so local smoke tests can exercise the same environment-aware
library APIs.

`parse_package_config` accepts in-memory `.dart_tool/package_config.json` content,
ignores extension fields as required by the format, validates version 2, package names,
RFC 3986 URI references, package-root containment, and language versions, and returns
structured diagnostics. Project inputs can attach zero or more configs with
`DartProjectInput::with_package_configs`.

`resolve_package_uri` resolves valid entries with RFC 3986 reference semantics. The URI
graph selects the nearest package config for each source file, resolves relative
`rootUri` and optional `packageUri`, and exposes both `target_uri` and a `target_path`
when the result belongs to the loaded project. Configured cache dependencies are
`resolved_external`; an invalid nearest config is reported and never bypassed with a
pubspec guess.

Validated `part` files participate in their owner's library namespace. Operations can
resolve between sibling parts as `same_library`, and importing the owner exposes public
operations declared in its parts. Missing, mismatched, or multiply claimed parts are
excluded instead of being assigned heuristically.

## Reference Strategy

DartScope behavior should be traceable to official Dart and Flutter sources first, with implementation and ecosystem tools used as practical references rather than sources of truth.

- [Reference strategy](docs/reference-strategy.md)
- [Library development plan](docs/development/dartscope-library-plan.md)
