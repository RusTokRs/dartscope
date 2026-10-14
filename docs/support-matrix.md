---
id: doc://docs/support-matrix.md
kind: support_policy
language: en
source_language: en
status: active
---

# DartScope 0.1 Support Matrix

This matrix defines what the `0.1` release line claims and, equally importantly, what it does not
claim.

## Rust And Host Platforms

| Area | Supported contract |
| --- | --- |
| Minimum Rust | Rust 1.95 |
| Pinned release toolchain | Rust 1.95.0 with rustfmt and Clippy |
| Edition | Rust 2024, Cargo resolver 3 |
| Hosted CI | Blocking workspace tests on `ubuntu-latest` and `windows-latest`, plus blocking macOS 15 arm64 portability and package-archive validation |
| CI Action runtime | Reviewed immutable Node 24 Action SHAs on GitHub-hosted runners compatible with Actions Runner 2.327.1 or newer |
| Release packaging | `ubuntu-latest` on exact Rust 1.95.0, with an independent macOS 15 arm64 archive build in the blocking CI matrix |
| macOS | Blocking hosted portability coverage on macOS 15 arm64, including workspace checks/tests and release package archives |

Rust versions older than 1.95 are unsupported. Newer stable compilers are expected to work, but the
MSRV and exact release gate are the compatibility anchors. The audited `0.2` development queue may
advance while package manifests remain on unreleased `0.1.0`; neither version is claimed as published
without its exact release tag. Self-hosted GitHub Actions runners are not supported until their runner
version and update policy are reviewed against `docs/development/ci-supply-chain.md`.

## Dart Language And Package Inputs

DartScope is source-only. It does not invoke the Dart SDK during normal library or CLI analysis, so
support is expressed by normalized capabilities rather than by claiming every construct from a
whole SDK release.

The `0.1` contract includes imports, exports, conditional URIs, parts, declarations, supported
members and locals, generic invocation facts, package configuration v2, namespace resolution,
GraphQL operation constants, and explicit diagnostics for known unsupported syntax. Dart 3.13
primary and concise constructors currently produce unsupported-syntax diagnostics rather than
fabricated declarations.

Records, patterns, complex cascades, complete lexical shadowing, type inference, overload-like
resolution, and a complete Dart AST are outside the `0.1` contract.

## Flutter

DartScope does not run `flutter` during normal analysis. The supported source conventions include:

- widget and application-route inventory;
- named `Navigator` calls and official `MaterialApp` / `WidgetsApp` route facts;
- `ThemeData` construction and supported theme application sites;
- literal asset and generated-localization catalog linking;
- explicit in-memory `l10n.yaml` and ARB inputs.

This is convention analysis, not widget-tree evaluation or Flutter type checking.

## Versioned Ecosystem Conventions

The opt-in ecosystem support table v1 uses these fixture contracts:

| Convention | Package | Supported constraint |
| --- | --- | --- |
| Go Router | `go_router` | `>=14.0.0 <18.0.0` |
| Provider | `provider` | `>=6.0.0 <7.0.0` |
| Riverpod | `flutter_riverpod` | `>=2.0.0 <4.0.0` |
| BLoC | `flutter_bloc` | `>=8.0.0 <10.0.0` |

A convention activates only when the caller opts in, a matching supported dependency is present,
and the analyzed file imports the package. These ranges do not imply complete API coverage for each
package.

## Serialized And CLI Contracts

Eight CLI command families use named v1 JSON envelopes, including `dartscope.lint-analysis`.
The lint command can alternatively emit SARIF 2.1.0. These are pre-1.0 contracts: incompatible changes
require a new schema major and a migration note. Library-only reference outputs are not command-facing
schemas until separately registered.

## LSP Bridge

`dartscope-lsp` is an optional, feature-gated editor bridge, not part of the default umbrella features.
The `0.1` contract covers the initialize/initialized/shutdown/exit lifecycle — requests arriving
before `initialize` answer `ServerNotInitialized`, requests after `shutdown` answer `InvalidRequest`,
and an `exit` without a preceding `shutdown` exits non-zero — full and incremental `didChange`
synchronization over the open document set, UTF-16 positions for LF, CRLF, and surrogate pairs, and
index-backed `definition`, `references`, `hover` (rendered as `dart` code blocks), `documentSymbol`,
and bounded `workspace/symbol` results, plus `textDocument/publishDiagnostics` for parser
diagnostics that echoes the client's document version. Queries landing on a declaration identifier
resolve onto the declaration itself (definition, hover, and references all work from the definition
site), `references` honors `ReferenceContext.includeDeclaration`, `didChange.version` is optional
per the 3.17 spec, and `didSave` revalidates and applies `includeText` payloads.

| LSP method | `0.1` status |
| --- | --- |
| `initialize` / `initialized` / `shutdown` / `exit` | lifecycle enforced: `-32002` before initialize, `-32600` after shutdown, exit status 0 clean / 1 abrupt |
| `didOpen` / `didChange` / `didClose` | supported; per-file index upsert; published diagnostics echo the last known `version` |
| `didSave` | revalidates; `includeText` content is applied as a full change |
| `publishDiagnostics` | pushed after every sync event; parser facts first, supplemental lint findings merged in range order |
| `definition` | supported; resolves from usage and from the declaration identifier (self) |
| `references` | supported; respects `includeDeclaration` |
| `hover` | `dart` code blocks per resolved target; declaration fallback included |
| `documentSymbol` | one nesting level; `detail` carries the declaration-kind label; enum children are `EnumMember` |
| `workspace/symbol` | open documents only, top-level names, bounded to 100 |
| anything else | honest `null` results; completion, formatting, semantic tokens, and rename are outside the contract | Index maintenance is per-file: opening or
changing a document upserts only that file, closing removes it, and unaffected open documents keep
their parse and resolution caches.

Built with the optional `lints` feature and started as `dartscope-lsp [root] --lints`, the stdio
binary drives the adapter itself: after each `didOpen`/`didChange` it runs the all-rules
`dartscope-lints` configuration over its own index snapshot and publishes the merged set. With
default features the library keeps its zero-dependency property and the port stays manual.

The bridge never scans the filesystem, never runs `dart` or `flutter`, and does not depend on
`dartscope-lints`. Lint findings reach the server only through `set_supplemental_diagnostics`, which a
downstream adapter fills. Completion, formatting, semantic tokens, inlay hints, and workspace-wide
background indexing are outside the `0.1` LSP contract, and every unhandled request returns an honest
empty result.
