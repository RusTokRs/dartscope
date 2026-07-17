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
| Hosted CI | `ubuntu-latest` and `windows-latest` for the normal workspace matrix |
| Release packaging | `ubuntu-latest` on exact Rust 1.95.0 |
| macOS | Expected to be portable Rust, but not a blocking hosted matrix for `0.1` |

Rust versions older than 1.95 are unsupported. Newer stable compilers are expected to work, but the
MSRV and exact release gate are the compatibility anchors.

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

Seven CLI command families use named v1 JSON envelopes. They are pre-1.0 contracts: incompatible
changes require a new schema major and a migration note. Library-only reference and lint outputs are
not command-facing schemas until separately registered.
