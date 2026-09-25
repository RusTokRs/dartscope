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
- Persistent per-library import/export dependency fingerprints with deterministic affected-library
  evidence for downstream incremental consumers.
- Snapshot-backed and incremental lint contexts that retain unaffected per-library diagnostics while
  preserving full stateless lint equivalence.
- Deterministic retained-cache payload metrics and informational 1k/10k index/lint update-time baselines
  without flaky absolute timing thresholds.
- Pinned RustSec advisory and unused-dependency CI gates with expiring, owner-attributed exception policy.
- Five nightly libFuzzer targets with reviewed malformed-input seeds and a bounded panic-free CI corpus.

### Changed

- macOS 15 arm64 portability and benchmark-regression jobs are blocking release gates alongside the
  Linux/Windows workspace matrix, workflow policy, RustSec, unused-dependency, and bounded-fuzz checks.
- Project traversal and command-facing path handling are deterministic across normalized duplicate
  inputs, deep directory trees, platform path separators, and supported symlink cases.

### Fixed

- Removed an unused direct `serde` dependency and stale lock edge from `dartscope-parse` instead of
  suppressing the unused-dependency gate.
- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
  visibility changes without leaking non-Dart metadata paths into `affected_paths`.
- Retained per-library namespace-membership and GraphQL-binding caches rebuild only affected GraphQL-use
  libraries while preserving the existing aggregate snapshot contract.
- Declaration navigation now preserves the correct member declaration span and owner filtering for
  static members, private owner types, tear-offs, and supported named-constructor forms.
- CLI filesystem reads remain bound to the validated target, project traversal is iterative, and
  normalized duplicate project inputs no longer produce duplicate analysis results.
- Loop lexical regions now retain exact reads, writes, and navigation through multi-declarator classic
  loops, existing-variable `for-in` targets, nested unbraced control statements, `try`/`on`/`catch`/
  `finally` bodies, and comments between chained clauses without leaking bindings after the loop.
- Unqualified member calls, reads, and writes inside a callable with one exact enclosing type now
  produce same-owner member facts only when the owner directly declares a matching method, field,
  getter, or setter and no visible lexical binding, parameter, local function, or enclosing-owner
  member shadows the spelling. Static-versus-instance evidence stays explicit, compound assignment and
  increment targets keep their paired read-then-write facts, and missing or shadowed spellings remain
  suppressed instead of fabricated.
- The flattened `version_or_source` dependency string is now a lossless round trip of the typed
  pubspec dependency source: values containing the `;` field separator are escaped on render and
  unescaped on flatten, scalar `git`/`hosted` shorthands rebuild their typed source including a sibling
  `version` key, and unknown `key=value` shapes degrade to the explicit `Other` variant.
- `dartscope-cli` project traversal now sorts collected directory entries before descending, so
  traversal-order diagnostics and traversal limits are deterministic across filesystems.
- Declaration inventory is no longer line-anchored: declarations that do not start their source line
  (one-line type bodies, second members or locals on a line, specifically typed and `late` top-level
  variables, declarations after masked comments) are collected with exact spans, while multi-line
  expression continuations never fabricate a declaration.
- Metadata annotations no longer hide their declaration. `@override int get x => 1;`,
  `@Deprecated('x') void f() {}`, annotated fields and locals, multi-line annotation arguments, and a
  declaration sharing its line with an annotation's closing parenthesis are all reported.
- Ordinary named factory constructors are collected as constructors instead of being skipped with a
  fabricated `unsupported_concise_constructor` warning; only the unprefixed Dart 3.13 concise forms
  emit that diagnostic, and declarations after them are still collected.
- Top-level `const`/`final` string constants now report the complete literal value and its exact span:
  triple-quoted and raw literals, literals spanning several lines, adjacent literal concatenation, and
  escaped quotes are no longer truncated to an empty or partial value, and a non-literal initializer
  such as `final value = readString('key');` is no longer reported as a string constant. Directive
  URIs use the same literal scanner.
- Dart identifiers are scanned with the language rule everywhere, including the dollar sign that
  generated and framework-facing sources use (`_$UserFromJson`, `UrlRequestCallbackProxy$Interface`,
  `jni$_`). Declaration names, import prefixes, combinators, type annotations, member and property
  references, invocations, and the naming lint share one canonical scanner instead of twelve local
  character classes, so `class Widget$Base` is no longer truncated to `Widget` and `count$` is no
  longer confused with `count`. GraphQL names keep their own dollar-free grammar.
- Unnamed `extension on T { ... }` declarations are reported instead of being dropped together with
  every member of their body. The declaration carries an empty name and a stable
  `<path>::extension:` symbol ID, its members keep it as parent, and the naming lint accepts the
  nameless declaration and any dollar-decorated name.

### Compatibility

- Minimum supported Rust version: 1.95.
- Workspace edition: Rust 2024 with resolver 3.
- Blocking host coverage: Linux, Windows, and macOS 15 arm64.
- Dart and Flutter support is capability-based and source-only; DartScope does not execute SDK
  tools during normal analysis.
- Existing command-facing JSON contracts remain at schema version v1.

Release notes remain under `Unreleased` until the exact version tag exists. The release process moves
this content to a dated version section and adds compare/release links in the same release operation.

[Unreleased]: https://github.com/RusTokRs/dartscope/commits/main
