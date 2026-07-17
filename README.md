# DartScope

DartScope is a standalone Rust toolkit for Dart and Flutter code intelligence.

It is a community-facing library ecosystem, not an Athanor adapter. Downstream tools can use DartScope for parsing, indexing, JSON export, CI checks, editor tooling, migration workflows, and graph adapters without depending on Athanor or Rustok-specific domain types.

## Current Scope

This repository is in early pre-1.0 development. The workspace bootstrap and first
file, project-index, package-resolution, JSON, CLI, and Flutter-inventory slices exist:

- `dartscope-core` owns normalized analysis types, spans, diagnostics, and pubspec models.
- `dartscope-parse` provides conservative source-only analysis for imports, exports, parts,
  declarations, generic invocation and named-argument facts, Dart-embedded GraphQL operations and
  uses, and structured `pubspec.yaml` discovery. It does not execute Flutter convention rules. The
  primary pubspec analysis preserves exact dependency-key and environment-key spans, normalizes
  scalar, SDK, path, git, hosted, and workspace sources, and embeds Flutter assets, flavors,
  platforms, ordered asset transformers, fonts, and localization-generation settings.
- `dartscope-index` performs project-level linking over normalized analysis results. Its
  first API resolves GraphQL operation uses conservatively and compares operation,
  client-call, and variable contracts without depending on parser internals.
- `dartscope-resolve` parses official package configuration v2 inputs and owns package
  and URI resolution primitives without performing filesystem I/O.
- `dartscope-flutter` derives widget, route, asset, and localization conventions from generic
  imports, declarations, and invocations, and aggregates project-level inventory. It is optional
  for pure Dart consumers and does not parse source directly.
- `dartscope-json` owns named versioned JSON envelopes and checked-in golden contracts;
  low-level generic Serde helpers remain available but are not stable command schemas.
- `dartscope-cli` exposes the stable process boundary with help, version output, documented exit
  codes, deterministic discovery, and versioned JSON for every analysis command.
- `dartscope` is a thin umbrella crate with feature-gated re-exports.

## Non-Goals

- Do not depend on Athanor crates.
- Do not expose parser-specific AST nodes as the main public API.
- Do not pretend heuristic findings are complete.
- Do not run `dart` or `flutter` commands during normal analysis.

## Current Limitations

- The first parser backend is conservative and does not provide a complete Dart AST or
  type system. Lexical masking prevents findings inside comments and strings, while a
  structural declaration pass records complete spans for supported declarations. Complex
  metadata layouts, patterns, records, and newer language-versioned syntax remain limited.
- Pubspec parsing uses a private `yaml-rust2` marked-event backend for dependencies,
  environment constraints, assets, fonts, selectors, and transformers. YAML aliases and merge keys
  remain explicitly unsupported, and selector validation follows the serialized DartScope v1 policy
  rather than claiming compatibility with every future Flutter SDK.
- Generic invocation discovery is conservative rather than a complete expression AST. It
  records dotted call targets, positional and named arguments, simple string values, map entries,
  result-member chains, enclosing callable IDs, and source evidence for supported forms. Complex
  cascades, records, patterns, and language-version-specific expressions can still be incomplete.
- Declaration inventory covers top-level declarations plus class, mixin, enum,
  extension, and extension-type methods, traditional constructors, fields, getters,
  setters, operators, and local variables. Declarations carry stable hierarchical symbol
  IDs and optional complete declaration spans. Dart 3.13 primary and concise constructors
  currently produce explicit diagnostics instead of heuristic symbols.
- CLI JSON uses named v1 envelopes and deterministic golden fixtures. Payload fields remain
  pre-1.0 contracts, so breaking changes require a new schema major and a migration note.

`dartscope-parse` also exposes an object-safe `DartParser` contract for callers that
need a different source-only parser backend. The built-in `HeuristicDartParser` remains
the default; capability metadata makes unavailable facts explicit. See
[`docs/development/parser-backends.md`](docs/development/parser-backends.md).

Pure `dartscope_parse::analyze_file` and `analyze_project` return generic Dart facts and leave the
legacy `DartFileAnalysis.flutter` compatibility projection empty. Applications that enable the
optional Flutter feature can call `dartscope::analyze_file_with_flutter` or
`dartscope::analyze_project_with_flutter`; the CLI uses those explicit composition APIs for its
file/project commands. `dartscope_flutter::extract_flutter_inventory` can also derive inventory
straight from a pure project analysis. See
[`docs/development/flutter-boundary.md`](docs/development/flutter-boundary.md).

## Rust Toolchain

DartScope requires Rust 1.95. The repository pins the exact Rust 1.95.0 toolchain in
`rust-toolchain.toml`, including rustfmt and Clippy. Every workspace crate inherits
`rust-version = "1.95"` and `edition = "2024"` from the root `Cargo.toml`. Because the
repository root is a virtual workspace, it explicitly declares Cargo resolver 3.

A dedicated CI matrix verifies resolver 3 and edition 2024 on Linux and Windows for the
complete workspace, the umbrella crate without default features, and the umbrella crate
with all features. See
[`docs/development/rust-2024-edition.md`](docs/development/rust-2024-edition.md).

## Quick Start

```powershell
cargo test --workspace
cargo run -p dartscope-cli -- --help
cargo run -p dartscope-cli -- --version
cargo run -p dartscope-cli -- analyze-file path\to\file.dart
cargo run -p dartscope-cli -- pubspec path\to\pubspec.yaml
cargo run -p dartscope-cli -- pubspec-config path\to\pubspec.yaml
cargo run -p dartscope-cli -- analyze-project path\to\flutter_project
cargo run -p dartscope-cli -- graphql-contracts path\to\flutter_project
cargo run -p dartscope-cli -- uri-graph path\to\flutter_project
cargo run -p dartscope-cli -- uri-graph path\to\flutter_project --env dart.library.io=true
cargo run -p dartscope-cli -- flutter-inventory path\to\flutter_project
```

`analyze-project` recursively scans regular `.dart` files and `pubspec.yaml` files,
never follows symlink entries, skips the documented generated/tool directory list, and
returns a deterministic JSON summary plus per-file analysis output. The CLI explicitly enables
optional Flutter convention composition, while the underlying pure parser remains Flutter-free.
Current output includes generic invocation and named-argument facts, top-level string constants,
GraphQL operation documents from Dart raw string constants, declared operation
variables, client uses such as `gql(operationConstant)` inside
`query`/`mutate`/`subscribe` calls, supplied client variable names, conservative Flutter
widget hints, `GoRoute` hints with `resolved_path` when a route path can be resolved
from same-file string constants, and high-confidence direct Flutter asset/localization
references such as `Image.asset(...)`, `AssetImage(...)`, `rootBundle.loadString(...)`,
`DefaultAssetBundle.of(...).loadString(...)`, and `AppLocalizations.of(context)!.key`.
Use it as the first real-project feedback loop before adding broader parser or Flutter
convention support. CLI success writes only JSON to stdout; argument and filesystem errors
write only to stderr with stable exit codes. See
[`docs/development/cli-contract.md`](docs/development/cli-contract.md).

File, pubspec, and package-configuration diagnostics include their normalized source
path. Byte spans account for both LF and CRLF input, so downstream evidence can use the
reported offsets without platform-specific correction. Pubspec dependency and
environment spans cover their key token rather than the complete source line.

Each `PubspecDependency` stores a typed `source` using `version`, `sdk`, `path`, `git`,
`hosted`, `workspace`, or `other` variants with a stable Serde `kind` discriminator.
The legacy `version_or_source` field remains beside it for pre-1.0 compatibility.
`structured_source()` returns the stored typed source and derives it from the legacy
field when reading an older payload that does not contain `source`.

`parse_pubspec` returns the complete primary model. Its `configuration` field contains
environment constraints and typed `uses_material_design`, `generate_localizations`,
asset paths, complete asset configurations, font families, font assets, styles, and
validated weights. The compatibility `assets` list retains path and span only;
`asset_configurations` adds optional flavors, optional platforms, and ordered transformer
packages with scalar arguments. The `pubspec` CLI command prints this migrated shape,
and pubspecs inside `analyze-project` use the same parser. Older JSON without
`configuration` or `asset_configurations` remains readable through Serde defaults.

`parse_pubspec_configuration` remains available as a focused configuration-only API.
The `pubspec-config` CLI command prints that structure as deterministic pretty JSON for
callers and smoke tests that do not need dependency discovery.

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
- [Rust code standards](docs/development/rust-code-standards.md)
- [Agent workflow](AGENTS.md)
- [Contributing](CONTRIBUTING.md)
