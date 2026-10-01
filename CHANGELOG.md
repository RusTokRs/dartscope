# Changelog

All notable changes to DartScope are documented in this file.

The format follows Keep a Changelog, and the project uses semantic versioning while remaining
pre-1.0.

## [Unreleased]

### Added

- Ten publishable Rust crates covering normalized Dart analysis, parsing, package and URI
  resolution, project indexing, optional lint rules, Flutter conventions, versioned JSON contracts,
  an optional language-server bridge, a thin umbrella API, and the `dartscope` CLI.
- Conservative source-only Dart and Flutter analysis with exact spans, diagnostics, capability
  metadata, namespace and reference resolution, GraphQL contract linking, and package-aware Flutter
  catalogs.
- Stable v1 CLI JSON envelopes, deterministic fixtures, explicit exit codes, and Linux/Windows
  process-level coverage.
- Versioned opt-in ecosystem conventions for `go_router`, Provider, Riverpod, and BLoC.
- Two end-to-end mutation tests, `dartscope-parse` and `dartscope-index` `robustness_mutations.rs`: deterministic random damage to realistic Dart sources must neither panic nor produce a span that does not describe the text, and an incrementally updated workspace index must equal a stateless analysis after every edit, removal and re-addition. Failures are grouped by source location and shrunk to a reproducer; the defaults are small and `DARTSCOPE_MUTATION_ROUNDS` and `DARTSCOPE_MUTATION_SEED` turn them up for a hunt (`docs/development/fuzzing.md`).
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
- Canonical Dart literal and metadata scanners (`crates/dartscope-parse/src/literals.rs` and `metadata.rs`) consolidating string, numeric and annotation character classes; `lexical.rs` and `invocations/arguments.rs` now share `identifiers::is_identifier_continue` (`$`) and `single_string_literal_value`.
- Incremental workspace index: the invalidation of dependents compares every declaration other files can see (all but local variables, with relations and both spans), not only the names, kinds and line spans of the top-level ones. A body that grew below the first line of its class, a member renamed to a name of the same length and a changed `extends` used to leave stale resolutions in the files that depend on the edited one; a randomized comparison of an updated index with a fresh one found this.
- Incremental workspace index fingerprints include declaration spans again. Cached cross-file resolutions embed the span of their target declaration, so a span-free fingerprint left dependents with stale spans and broke the snapshot-equals-stateless contract; a formatting-only edit therefore invalidates the files that depend on the edited one.
- Direct inherited instance members via `extends`/`with` (and `mixin on`) and extension member fallback without receiver inference.
- `DartDeclaration.on_types` (additive; omitted from JSON when empty) holds the `on` constraints of a mixin and the `on` type of an extension. It is empty for an extension whose `on` type is one of its own type parameters, which applies to every receiver.
- Populated v1 golden fixtures (`file-analysis-populated-v1.json` / `project-analysis-populated-v1.json`) with one file, one import and one declaration to catch regressions inside entry objects.
- New optional crate `dartscope-lsp` with UTF-16 ↔ UTF-8 coordinate conversion (`\\n`, `\\r\\n`, `\\r`, surrogate pairs; `LineIndex` answers each conversion with a binary search), incremental `DartLspServer` (initialize, didOpen/didChange/didClose, definition, references with `includeDeclaration`, hover, documentSymbol, diagnostics), a transport-independent `rpc` module (`serve`, `handle_message`, `read_frame`, `write_message`) and a minimal stdio binary `dartscope-lsp`.
- `dartscope analyze-project` leaves a `.dart` file that is not valid UTF-8 out of the analysis and reports it as the warning `input_file_not_utf8` (with the file path, counted in `summary.diagnostics`) instead of failing the whole run; every other command keeps rejecting it with exit code 3.
- `dartscope.orphan_file` reports each configured entry point that is not an analyzed Dart file, and `dartscope lint` rejects an enabled `orphan_file` rule with an empty `entry_points` list (exit code 5). Both used to turn the rule silently into a no-op.
- `--compact` for every `dartscope` command prints the JSON (and SARIF) document on one line; the default output stays indented. `analyze-project --relative-root` reports `data.root` as `.` so the document does not depend on the checkout location, and `analyze-project --skip-symlinks` reports a symlink that the walker rejects as the warning `input_symlink_skipped` instead of failing the run.
- `analyze-project` reports each `build`, `coverage`, `target`, `Pods`, `node_modules`, `.symlinks` and `.plugin_symlinks` directory the walk did not enter as an `input_directory_skipped` diagnostic of severity `info` (additive, counted in `summary.diagnostics`), so that a missing file is explained instead of silent.
- Lint configuration, all additive and defaulting to the earlier behavior: `path_match = "segment"` compares every configured path prefix by whole path segments (`lib/ui` no longer covers `lib/ui_kit/`), a forbidden import pattern can use `match_kind = "segment_prefix"` (`package:flutter` no longer matches `package:flutter_bloc`), and `[exclude] path_prefixes`/`path_suffixes` drop the findings about generated files such as `.g.dart` for every rule. The library API gains `DartLintPathMatch`, `DartLintExclusions`, `DartLintConfig::is_excluded` and `DartImportPatternKind::SegmentPrefix`. An error diagnostic about an excluded file (a template `pubspec.yaml` with placeholders) no longer ends `dartscope lint` with exit code 6.

- Flutter widgets are found through project superclasses: `class Screen extends BaseScreen`, where `BaseScreen extends StatelessWidget` is declared in the same or another file, is a widget with confidence `medium`. `FlutterWidgetHint` and `FlutterWidgetEntry` gain the additive `inherited_via` (the project class named in `extends`; omitted for a direct subclass), and `base_class` of such a widget is the Flutter base it reaches. The chain is followed by class name at any depth; a private name is looked up in its own file only and a public name that two classes declare is not followed (`docs/development/flutter-boundary.md`). `imports_official_flutter` is evaluated once per file instead of once per invocation.

- Per-file analysis budgets (`docs/development/json-contracts.md`, "Analysis budgets"). Invocation facts copy source text (a call chain copies its dotted prefix into every call's target, a nested call is copied into each enclosing call's arguments), so the facts of 64 KiB of nested calls were 716 MB. The copied text of a file is now limited to 32 times its size plus 1 MiB for targets and the same for arguments, and the declaration inventory may read 32 times the file size plus 1 MiB in its unbounded scans; a file that exceeds a budget keeps the facts found before it and gains the warning `invocation_facts_truncated` or `declaration_scan_truncated` with the line where it stopped. Both budgets are counted in bytes and ordinary code stays far below them.

- `dartscope-lsp` loads the project instead of only the open documents (`docs/development/lsp.md`). When the client sends `initialized`, the `dartscope-lsp` binary reads every Dart source, `pubspec.yaml` and `.dart_tool/package_config.json` under the workspace folders with the skip rules of `analyze-project` and the server analyzes them in one pass; an open document takes the place of its file, and the file returns when the buffer closes. Definition, references and hover now reach files that are not open, `package:` imports resolve, `workspace/symbol` searches the project (new capability `workspaceSymbolProvider`), and `workspace/didChangeWatchedFiles` keeps it current (the server registers the watchers with clients that support dynamic registration). New library API: `WorkspaceSource`, `WorkspaceFile`, `WorkspaceScan`, `NoWorkspace`, `FsWorkspace`, `rpc::serve_with` and `rpc::handle_message_with` (`serve` and `handle_message` keep their behavior and read nothing), `DartLspServer::load_workspace`, `update_workspace_file` and `workspace_symbols`, and `Url::from_file_path`. The limit for navigation over a document is 1 MiB, up from 256 KiB; a file on disk above it is not loaded.

### Changed

- `dartscope lint --config` rejects a configuration that enables no rule (exit code 5) instead of running a check that can report nothing; without `--config` the command stays inert. `analyze-project` reports `data.root` without `.` components (`/work/app`, not `/work/app/.`).

- `analyze_file` no longer takes quadratic time on broken or hostile input (audit 2026-09-30, section 16.2). The invocation scanner asked a counting scan "where does this `(` or `<` close?" for every candidate, which read to the end of the file for each unclosed opener, so 1 MiB of `a(` took minutes; one pass now pairs the delimiters and each question is a binary search. A run of annotation lines asked for the end of the whole run from each of them, the end of a braceless statement was measured from every enclosing `for`, and a `for` nest thousands of levels deep needed one stack frame per level; both are measured once now. Measured on Linux in release mode (64 KiB, then 1 MiB; before: 64 KiB, then the size at which it passed 10 s): unclosed `a(` 740 ms, over 10 s at 256 KiB, now 13 ms and 258 ms; nested calls 3 s, now 27 ms and 372 ms; a chain of 16,000 calls 2 s, now 35 ms and 421 ms; 21,000 annotation lines 660 ms, now 12 ms and 179 ms; nested braceless `for` statements 610 ms, now 21 ms and 351 ms. Results for ordinary input are identical; each lookup has an equivalence test against the scan it replaces.
- `analyze_file_with_references` and `analyze_project_with_references` are linear in the size of a file (audit 2026-09-30, section 10). Every reference pass (reads, writes and updates of bindings, invocation targets, member, property and operator references, lexical regions and bindings) compared each identifier with all declarations, bindings, references and regions of the file; they now answer from lookup structures built once per file (`FileFacts`: declaration tables, interval indexes over spans, the statement and delimiter structure of the masked text). The result is identical, not approximately equal: replaying 15,652 damaged sources against the previous implementation produced byte-identical analyses, and each structure has an equivalence test against the scan it replaces. Measured on Linux in release mode, the passes after `analyze_file` took 27.5 s for 4,000 classes in one file and now take 0.09 s; one method with 8,000 local variables went from 51 s to 0.11 s, one expression with 8,000 children from 15 s to 0.05 s.
- **Declaration inventory contract** (audit 2026-09-30): `DartDeclaration.extends` is the `extends` clause of a class and nothing else (an extension's `on` type used to be stored there); `mixes_in` holds the `with` types of classes and enums (a mixin's `on` constraints used to be stored there); `on_types` holds both kinds of `on` types. Enum constants are `Field` declarations whose parent is the enum. A top-level `get`/`set` is a `Getter`/`Setter` declaration instead of being dropped. `implements` is not modeled, so lookup through an `implements` clause stays a known false negative.
- Flutter widget, state and route findings are produced for `Class` declarations only; an `extension` or `mixin` whose name resembles a widget is no longer reported as a widget.
- The CLI directory walker always skips `.dart_tool`, `.git`, `.idea`, `.pub-cache`, `.vscode`, `.symlinks`, `.plugin_symlinks`, `Pods` and `node_modules`. `build`, `coverage` and `target` are skipped only outside the source roots (`lib`, `bin`, `test`, `test_driver`, `tool`, `integration_test`, `benchmark`), so sources in `lib/**/build` are analyzed. `.symlinks` and `.plugin_symlinks` hold the links that `pod install` and Flutter create, which previously made `analyze-project` and `lint` fail with `input_symlink_rejected`.
- `dartscope.naming_convention` does not judge names that contain `$`; `dartscope.forbidden_import` also checks `export` directives and conditional-import alternatives; `dartscope.layer_boundary` also checks `export` targets (a `part` stays inside its library).
- SARIF artifact URIs are percent-encoded, and a result without a source span gets a line-1 region so that code scanning accepts it.
- `dartscope-lsp` advertises its capabilities and reads `initialize` params with the protocol's camelCase names, answers every request (unknown method `-32601`, invalid params `-32602`, before `initialize` `-32002`, after `shutdown` `-32600`), exits with 0 after `shutdown` and `exit` and with 1 otherwise, and caps a message body at 64 MiB. Results name documents with the URI the client sent, and a symbol that lives in a document that is not open is skipped instead of being located against the requester's text. A panic inside a handler answers the request with an internal error (`-32603`) instead of ending the session, and a document whose analysis panics is left out of the index with the warning `analysis_failed` until its text changes. A document larger than 256 KiB keeps its outline and diagnostics but is left out of navigation, with the information diagnostic `navigation_disabled_large_file`, because reference analysis grows faster than linearly with the size of one file (about 0.8 s for 330 KB and 25 s for 1.3 MB) and runs again after every edit.
- macOS 15 arm64 portability and benchmark-regression jobs are blocking release gates alongside the
  Linux/Windows workspace matrix, workflow policy, RustSec, unused-dependency, and bounded-fuzz checks.
- Project traversal and command-facing path handling are deterministic across normalized duplicate
  inputs, deep directory trees, platform path separators, and supported symlink cases.
- `dartscope-cli` project traversal now uses a `VecDeque` breadth-first queue with per-directory sorted `entries` so `max_pending_directories` and `max_directory_entries` diagnostics are lexicographically deterministic.

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
- `lexical.rs:is_identifier_byte`, `member_references::is_identifier_continue` and `has_constructor_keyword`, `declaration_inventory/scanner.rs:annotations_end`, and `flutter/conventions.rs` interpolation now include `$` so `foo$r'bar'`, `count$`, `@_$Annotation`, `my$const` and `_$kAssetBase` are handled like `identifiers.rs`.
- `pubspec_yaml_marked.rs` no longer panics on malformed mappings with a missing key; the malformed input now produces a `pubspec_invalid_yaml` diagnostic and keeps the fuzz corpus panic-free.
- Restored a compiling workspace that passes `clippy -D warnings` (audit 2026-09-30): the missing `dartscope-lsp` lock entry, an invalid `let … else` in `pubspec_yaml_marked.rs`, dead helpers, a wrong expectation in a `literals.rs` test, three `dartscope-lsp` tests with wrong expectations, and the crate count in the CI policy scripts (nine became ten).
- `analyze-file` and `analyze-project` are linear in the size of the input again. A span rebuilt the line table of its file, a call was classified against every declaration of the file, a member or local scan kept walking over the rest of the file after the end of its body, the column and brace depth of a declaration were measured from the start of its line, and a statement with thousands of argument lines was rescanned from each of them. Measured on CI runners: a file of 16,000 classes went from 108 s to 0.3 s, 80,000 functions from 7.7 s to 0.4 s, 200,000 call-argument lines from a timeout to 1.5 s, the 2.7 MB `pedometer_bindings_generated.dart` from more than 150 s to 0.3 s, and `analyze-project` over the 484 files of `flutter/samples` from 223 s to 0.6 s. A line table is built once per file, columns come from a binary search over the continuation bytes of multi-byte characters, and regression tests count scanned bytes and visited lines instead of timing anything.
- The analysis no longer panics on a non-ASCII character in code that is not valid Dart, which an editor shows while the code is being typed: `é(` and `for(é)` sliced a keyword at a byte that is not on a character boundary (found by the mutation tests).
- A leading UTF-8 byte-order mark is a preamble of the file, not source text: it no longer hides the first declaration or directive, shifts columns, or breaks `pubspec.yaml`. An empty `flutter:` section of `pubspec.yaml` is accepted, as the Flutter tool accepts it.
- The declaration inventory no longer fabricates or loses declarations: a function or method with a generic or function-typed result, a top-level getter or setter, enum constants, named arguments such as `padding: padding` (they were reported as local variables), and `${…}` interpolation containing quotes or braces are handled; the scan of an interpolation is bounded in length and nesting.
- Navigation follows Dart's lookup order through any depth of `extends` and `with` (with a depth bound and cycle protection) instead of one level; an extension member is offered only when the extension is visible (imported with or without a prefix, not deferred, not hidden) and its `on` type fits the receiver, and the interface member always wins. Member lookups use hash indexes instead of scanning every declaration.
- `uri-graph` reads a relative `import`, `export` or `part` URI as a URI: percent escapes are decoded (`'b%20c.dart'` names `b c.dart`). An empty or blank URI, an escaped path separator (`%2F`), a relative path that climbs out of the project root and a `package:` URI that climbs out of its library directory (`package:app/../secret.dart`) are `invalid_uri` instead of a missing file; dropping the surplus `..` segments used to link `lib/a.dart` importing `'../../x.dart'` to the `x.dart` at the project root.
- `dartscope` no longer panics on an argument that is not valid Unicode (exit code 2) or on a closed stdout (`dartscope … | head` keeps the exit code of the command).
- `dartscope-lsp` was not usable by an editor: its capabilities and `initialize` parameters used snake_case names, so no client saw a feature and `rootUri` was never read; malformed or unknown requests were dropped without an answer; `exit` always returned 0; the body size was trusted for an allocation; `publishDiagnostics` was never sent; percent-escapes were decoded byte-by-byte into the wrong characters; `didChange` with a range outside the document replaced the whole text; `selectionRange` was the whole source line; the index was rebuilt from every open document on every keystroke; and results for another document were converted against the text of the requesting one.

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
