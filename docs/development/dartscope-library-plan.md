---
id: doc://docs/development/dartscope-library-plan.md
kind: development_plan
language: en
source_language: en
status: active
---

# DartScope Library Development Plan

This is the executable roadmap for the standalone DartScope Rust workspace at
`D:\DartScope`. It records verified implementation state, architectural boundaries,
ordered tasks, acceptance criteria, and commands that an implementation agent must run.

DartScope is a community-facing Dart and Flutter code-intelligence toolkit. It is not
an Athanor adapter and must not depend on Athanor crates or Athanor domain types.

## How To Use This Plan

An agent should:

1. Read `AGENTS.md`, `README.md`, this plan,
   `docs/development/rust-code-standards.md`, and `docs/reference-strategy.md`.
2. Select the first `ready` task in the Ordered Work Queue.
3. Read the task's files and official references before editing.
4. Add a failing focused test or fixture that reproduces the task.
5. Implement only the behavior required by that task.
6. Update this plan and user-facing docs in the same change.
7. Run the task checks and the repository Definition Of Done.

Do not start a later task because it looks easier while an earlier `ready` task is
unfinished. A task may be skipped only when it is marked `blocked` with a concrete
reason in this file.

The complete workspace uses the repository-pinned Rust 1.95.0 toolchain, Rust edition
2024, and Cargo resolver 3. Workspace packages inherit `rust-version = "1.95"` and
`edition = "2024"`; CI, rustfmt, Clippy, rustdoc, tests, dependency reviews, and
parser-backend decisions must not target a second Rust version, edition, or resolver.

Status vocabulary:

| Status | Meaning |
| --- | --- |
| `verified` | Implemented and covered by the listed repository checks |
| `implemented` | Code exists, but one or more acceptance checks are still missing |
| `in_progress` | A bounded part exists and the remaining work is listed |
| `ready` | Defined well enough for the next agent to implement |
| `planned` | Ordered later; prerequisites are not complete |
| `blocked` | Cannot proceed until the named blocker changes |
| `research` | Investigation is recorded; implementation scope and acceptance are not yet committed |
| `deferred` | Explicitly outside the current release target |

## Product Boundary

DartScope owns:

- Dart source and package analysis;
- source paths, spans, diagnostics, confidence, and normalized analysis types;
- imports, exports, parts, declarations, packages, and project relationships;
- optional Dart-embedded GraphQL discovery and contract analysis;
- optional Flutter conventions and inventory;
- optional lints over normalized analysis;
- Rust APIs, CLI workflows, and general-purpose JSON output.

DartScope does not own:

- Athanor entities, evidence, ownership, stable keys, or query contracts;
- Rustok-specific route manifests or product rules;
- a general graph database or visualization frontend;
- implicit execution of `dart`, `flutter`, build scripts, or network requests;
- claims of analyzer-equivalent type checking from the heuristic backend.

The Athanor integration plan remains in
`D:\Athanor\docs\development\dart-flutter-adapter-plan.md`. Athanor may consume a
stable DartScope API, but DartScope must never import Athanor.

## Source And Evidence Policy

Every supported construct must be classified as one of:

- `normative`: official specification, language docs, framework API docs;
- `behavioral`: observed output from official Dart, pub, analyzer, or Flutter tools;
- `implementation`: parser grammar or analyzer implementation detail;
- `ecosystem`: package convention such as `go_router` or Riverpod;
- `heuristic`: a DartScope approximation with explicit limits;
- `consumer`: a downstream need that may motivate output but cannot define semantics.

Normative and behavioral sources define semantics. Ecosystem and heuristic support must
include confidence metadata or diagnostics, a positive fixture, and a nearby negative
fixture. The living source map is `docs/reference-strategy.md`.

## Verified Baseline

Baseline reviewed on 2026-07-16.

| Area | Status | Evidence in repository |
| --- | --- | --- |
| Rust workspace and eight crates | verified | root `Cargo.toml`; exact Rust 1.95.0 Linux/Windows quality, test, edition, and feature matrix passed |
| Core normalized model | implemented | declarations, generic invocations, spans, diagnostics, and compatibility projections; pre-1.0 migration work remains |
| File and pubspec analysis | in_progress | heuristic declarations and generic invocations plus marked `yaml-rust2` pubspec backend; unit and project fixtures |
| Package config v2 and package URI resolution | in_progress | `dartscope-resolve`, six resolver tests |
| URI graph, parts, and GraphQL linking | in_progress | `dartscope-index`, deterministic JSON contract tests |
| Flutter project inventory | verified | optional convention derivation and deterministic inventory behind the `flutter` feature |
| Versioned JSON contracts | verified | seven named v1 command envelopes, golden fixtures, and migration policy |
| CLI process contract | verified | help, version, exit codes, deterministic discovery, and Linux/Windows process tests |
| Hosted CI | verified | Rust 1.95.0 quality, Linux/Windows tests, edition-2024, and umbrella feature matrix publish granular and aggregate statuses |
| Contributor and agent workflow | verified | `AGENTS.md`, `CONTRIBUTING.md`, Rust code standard |
| Lint/rule engine | planned | crate not created |
| Parser backend port | verified | `DartParser` capability contract, default heuristic backend, injection path, and backend documentation |

Current verified behaviors include:

- normalized LF/CRLF byte spans and source paths on diagnostics;
- imports, exports, namespace combinators, conditional URIs, parts, and part ownership;
- top-level and supported member/local declaration inventory with stable ownership and full spans;
- generic invocation targets, named and positional arguments, map entries, result-member chains,
  enclosing callable IDs, and source evidence;
- pubspec dependency sections with flexible direct-child indentation;
- typed pubspec dependency sources, environment constraints, fonts, and Flutter asset
  configurations with paths, flavors, platforms, and ordered transformers;
- package configuration v2 parsing and nearest-config package URI resolution;
- deterministic project ordering, URI graphs, and GraphQL contract results;
- direct and re-export GraphQL visibility, part-library membership, client-call and
  variable compatibility;
- high-confidence widget, route, asset, and localization hints;
- optional Flutter convention derivation and deterministic inventory that preserve route path
  kind, confidence, paths, spans, and ordering.

## Known Architectural Debt

The following are known facts, not hidden assumptions:

1. `dartscope-core` still contains the legacy `DartFileAnalysis.flutter` projection for
   v1 payload compatibility. Pure parsing leaves it empty and optional composition populates it;
   removing it requires a future Rust and JSON migration.
2. Generic invocation discovery is conservative rather than a complete expression AST. Complex
   cascades, annotations, records, patterns, and recent language-version-specific expressions can
   create misses and must remain explicit capability limits.
3. Command envelopes are versioned, but their payloads still serialize public domain structs
   directly. Pre-1.0 field evolution therefore requires additive Serde defaults, golden updates,
   and migration notes.
4. Project diagnostics carry paths, but diagnostic codes do not yet have a public registry
   documenting severity, source class, and stability.

Do not conceal these limits by changing README wording. Close them through the tasks
below.

## Target Architecture

```text
filesystem / editor / caller
          |
          v
  input adapters and CLI
          |
          v
  parser backend port ---- optional official analyzer bridge
          |
          v
 normalized Dart syntax and semantic facts
       |          |             |
       v          v             v
   resolver    project index   optional Flutter conventions
       |          |             |
       +----------+-------------+
                  |
                  v
       optional lints and JSON adapters
```

Crate responsibilities:

| Crate | Responsibility | Must not do |
| --- | --- | --- |
| `dartscope-core` | stable generic inputs, spans, diagnostics, normalized contracts | I/O, parser backend logic, consumer mapping |
| `dartscope-parse` | parser port, heuristic backend, normalized Dart facts | project graph policy, Athanor mapping |
| `dartscope-resolve` | package config and URI/symbol resolution primitives | filesystem scans, source parsing |
| `dartscope-index` | deterministic cross-file and package analysis | parse raw Dart source |
| `dartscope-flutter` | optional Flutter and supported ecosystem conventions | pure Dart semantics, Athanor entities |
| `dartscope-lints` | optional rules over normalized models | raw source parsing |
| `dartscope-json` | versioned serialization boundary | business logic |
| `dartscope-cli` | filesystem/process adapter and human workflows | own analysis semantics |
| `dartscope` | feature-gated re-exports and high-level composition | duplicate implementation |

## Public Contract Rules

- Paths use `/` separators in public output.
- Byte spans are zero-based half-open ranges. Lines and columns are one-based.
- Findings retain their source path either through the containing analysis or directly
  where they can appear in project-level output.
- Project and inventory outputs are sorted deterministically inside library APIs, not
  only by the CLI scanner.
- Heuristic output includes `Confidence` or a diagnostic.
- Unresolved references remain explicit; absence from the loaded index is not proof
  that an external package or file is invalid.
- New serialized fields should use backwards-compatible Serde defaults when possible.
- Renames or removals require a schema migration note and fixture update.

## Ordered Work Queue

### DS-MAINT-001: Split Oversized Parse And Index Modules

Status: verified. Priority: P0. Owner crates: `dartscope-parse`, `dartscope-index`.

Problem:

The production roots currently combine too many responsibilities:

- `dartscope-parse/src/lib.rs` is about 2,386 lines;
- `dartscope-index/src/lib.rs` is about 2,013 lines;
- `analyze_file` is 115 lines with Clippy cognitive complexity 26;
- `analyze_graphql_contracts_with_options` is 154 lines.

These exceed the mandatory refactor triggers in
`docs/development/rust-code-standards.md`. Adding lexical recovery or broader symbol
resolution directly to these roots would make ownership less clear.

Result:

- `dartscope-parse/src/lib.rs` is a thin crate boundary with private modules for analysis,
  declaration inventory, generic invocations, GraphQL, namespace directives, pubspec, and source
  lines;
- `dartscope-index/src/lib.rs` is now a 13-line crate boundary with private URI graph,
  part, GraphQL, and path modules;
- tests are split by behavior under each crate's `src/tests/` directory;
- public re-exports, JSON shapes, fixtures, diagnostics, confidence, paths, and ordering
  remain unchanged;
- the selected Clippy maintainability audit is clean, including test targets.

Required work:

1. Add or retain characterization tests before moving implementation.
2. Split `dartscope-parse` by stable responsibility, targeting modules such as
   `source_lines`, `namespace`, `declarations`, `declaration_inventory`, `invocations`, `graphql`,
   and `pubspec`; optional Flutter interpretation belongs in `dartscope-flutter`.
3. Split `dartscope-index` into URI graph, part membership, GraphQL visibility/contracts,
   and shared namespace resolution modules.
4. Keep crate-root public functions and public paths compatible through thin wrappers
   or re-exports.
5. Reduce orchestration functions below Clippy's 100-line and 25-complexity defaults;
   target 60 lines and complexity 20 where the stage boundaries remain meaningful.
6. Move unit tests next to their owning private modules or split integration tests by
   behavior. Do not create `utils`, `common`, numbered, or catch-all modules.
7. Do not change Rust output, JSON shape, diagnostics, confidence, ordering, or paths in
   this task.

Acceptance:

- each new module has one domain responsibility and a clear private/public boundary;
- `lib.rs` files contain crate docs, module declarations, re-exports, and thin
  orchestration rather than the complete implementation;
- all existing tests pass without fixture-output changes;
- the selected maintainability audit reports no production function above 100 lines or
  cognitive complexity 25:

```powershell
cargo clippy -p dartscope-parse -p dartscope-index --all-targets --locked -- `
  -W clippy::too_many_lines `
  -W clippy::cognitive_complexity `
  -W clippy::too_many_arguments `
  -W clippy::type_complexity
```

- the full Definition Of Done passes.

### DS-PARSE-004: Lexical Masking And Recovery

Status: verified. Priority: P0. Owner crate: `dartscope-parse`.

Problem:

The line-oriented backend can report declarations, imports, Flutter calls, and route
hints from block comments or string bodies. Broadening more heuristics before this is
fixed increases false positives.

Required work:

1. Add a backend-internal lexical pass that distinguishes code, line comments, block
   comments, normal strings, raw strings, and triple-quoted strings while preserving
   original byte offsets.
2. Feed masked code to line heuristics while retaining original text for spans and
   GraphQL document extraction.
3. Emit `unterminated_block_comment` and `unterminated_string` diagnostics with path
   and best available span.
4. Add positive and negative LF/CRLF tests.

Acceptance:

- `class`, `import`, `GoRoute`, `Image.asset`, and `AppLocalizations.of` text inside
  comments or unrelated strings produces no finding;
- real directives and declarations around comments retain exact spans;
- GraphQL raw triple strings still produce GraphQL operations;
- no parser-specific token type leaks into `dartscope-core`.

Required references: Dart lexical rules and string documentation. Required checks:
`cargo test -p dartscope-parse --quiet`, then full Definition Of Done.

### DS-PARSE-005: Parser Backend Port

Status: verified. Priority: P0. Prerequisite: DS-PARSE-004.

Required work:

1. Define an object-safe parser capability contract without filesystem I/O.
2. Wrap the current heuristic parser as the default backend.
3. Add capability metadata such as declarations, directives, members, recovery, and
   language-version coverage.
4. Keep existing convenience functions source-compatible where practical.
5. Document how a future tree-sitter or official analyzer bridge plugs in.

Acceptance:

- the existing fixture suite runs through the default backend;
- consumers can inject a backend without depending on backend AST types;
- unsupported capabilities are explicit rather than silently empty.

Implementation note: `DartParser` and `DartParserMetadata` live in `dartscope-parse`;
`HeuristicDartParser` is the default behind the existing convenience functions.
Alternative backends map their internal syntax trees to `dartscope-core` facts through
`analyze_project_with_parser`. See `docs/development/parser-backends.md`.

### DS-PARSE-006: Complete Declaration Inventory

Status: verified. Priority: P1. Prerequisite: DS-PARSE-005.

Implemented slices:

1. Normalized methods, traditional constructors, fields, getters, setters, operators, and
   local variables for class, mixin, enum, extension, and extension-type bodies.
2. Deterministic hierarchical `symbol_id` and `parent_symbol_id` values for top-level,
   member, and local declarations.
3. Additive optional `declaration_span` values covering the complete supported declaration;
   the existing `span` remains the compatibility anchor for the declaration's source line.
4. Multiple fields in one declaration and both inferred and explicitly typed local variables.
5. Body-depth filtering that excludes constructor calls and other expressions from the
   declaration inventory.
6. Explicit `unsupported_primary_constructor` and `unsupported_concise_constructor`
   diagnostics for Dart 3.13 syntax until language-version-aware parsing is implemented.

Verification:

- focused fixtures cover every required owner and member category, full spans, stable parents,
  local ownership, multiple fields, typed locals, and nearby constructor-call negatives;
- exact Rust 1.95 workspace tests, all-feature tests, formatting, and Clippy with warnings denied
  pass before finalization;
- the finalization workflow repeats the repository checks before committing to `main`.

Acceptance:

- fixtures cover class, mixin, enum, extension, and extension-type members;
- declarations have stable parent relationships;
- constructor calls are not reported as declarations;
- unsupported recent syntax emits a diagnostic rather than a fabricated symbol.

### DS-PUB-002: Structured Pubspec Model

Status: verified. Priority: P1. Owner crates: `dartscope-core`, `dartscope-parse`.

The primary pubspec model stores typed dependency sources, environment constraints, fonts,
and complete Flutter asset configurations with paths, flavors, platforms, ordered
transformers, compatibility defaults, source spans, and serialization fixtures. The
`yaml-rust2` marked-event implementation is the sole runtime dependency/configuration backend;
its parser types remain private to `dartscope-parse`.

Implemented slices:

1. Core-owned typed dependency sources for version, SDK, path, git, hosted, workspace, and
   fallback values, while retaining `version_or_source` for pre-1.0 compatibility.
2. Core-owned environment, Flutter generation, assets, and fonts embedded in the primary
   `PubspecAnalysis` model with Serde defaults for older payloads.
3. Wildcard-versus-alias handling, malformed-flow recovery, indentation diagnostics, quote
   balancing, and path-attributed invalid-YAML diagnostics.
4. Flutter asset `path`, `flavors`, `platforms`, and ordered `transformers` with optional
   scalar arguments, plus the compatibility path-only `assets` projection.
5. `yaml-rust2 = "=0.11.0"`, a Cargo-generated lock graph, a private marked-event AST, and
   domain conversion for configuration and dependencies.
6. Pre-cutover dual-backend parity for complete and focused APIs, including CRLF, Unicode byte
   evidence, duplicate keys, malformed inputs, aliases, and source normalization.
7. Public complete and focused APIs switched to the marked backend without changing public
   models, compatibility fields, diagnostic paths, or ordering.
8. Exact Rust 1.95.0 quality, tests, rustdoc, and edition/feature checks passed locally and on
   hosted Linux/Windows runners. Commit `566edbb0da58799d227a4615713631aefaf25978`
   received all granular success statuses and aggregate `dartscope/ci` success.
9. Conservative dependency/configuration/asset runtime modules removed after hosted cutover
   verification; representative former parity inputs remain explicit marked contract tests.
10. `flutter.default-flavor` normalized, selector validation exposed as
    `PubspecFlutterAssetSelectorPolicy::V1`, and older JSON defaulted to no default flavor plus
    policy `v1`. Commit `88e65e3c017b58ec9b64907efdeaa0e8d2ee67af` passed every hosted Rust 1.95.0
    Linux/Windows quality, test, edition, and feature context plus aggregate `dartscope/ci`.

Acceptance:

- indentation and comments do not change dependency identity;
- nested `sdk`, `path`, `git`, and `hosted` fields are attached to their dependency and never
  emitted as packages;
- malformed YAML produces a path-attributed diagnostic;
- serialization fixtures cover every dependency source and complete pubspec variant;
- structured Flutter assets preserve selectors, transformer order, arguments, and spans;
- `flutter.default-flavor` is normalized and selector validation is governed by an explicit
  serialized policy version;
- public pubspec APIs use the private marked backend without exposing `yaml-rust2` types;
- all focused and workspace checks pass on Rust 1.95.0 on Linux and Windows.

Implementation state and remaining limits are recorded in
`docs/development/pubspec-model.md` and `docs/development/yaml-backend.md`.

### DS-RESOLVE-003: Package Config Completeness

Status: verified. Priority: P1. Owner crate: `dartscope-resolve`.

Implemented slices:

1. Preserve `generated`, `generator`, and `generatorVersion`, validate the UTC timestamp
   and Semantic Version formats, and retain unknown extension fields by ignoring them.
2. Resolve roots and package-URI directories to canonical scheme/authority and decoded path
   segments before cross-entry comparison.
3. Reject duplicate roots and both normative package-URI/nested-root overlap directions while
   allowing nested package roots whose accessible package directories remain disjoint.
4. Reject literal and percent-encoded traversal or encoded path separators in `packageUri`
   and incoming `package:` URI paths.
5. Cover relative and absolute file URIs, percent escapes, nested roots, external cache roots,
   and Windows `file:///C:/...` paths.

Invalidation policy:

- any error diagnostic invalidates the complete package configuration, and
  `resolve_package_uri` returns `InvalidConfiguration`;
- optional generator metadata format problems are warnings and do not block resolution;
- extension fields remain ignored for forward compatibility.

Acceptance:

- official package-config v2 examples parse;
- every normative invalid case has a stable diagnostic code and test;
- resolution never escapes a package root or synthetic project root;
- exact Rust 1.95 quality, tests, rustdoc, edition, and feature checks pass on Linux and Windows.

### DS-JSON-001: Versioned JSON Contracts

Status: verified. Priority: P1. Owner crates: `dartscope-json`, `dartscope-cli`.

Implemented slices:

1. Define a seven-family `JsonContract` registry and stable `{schema, version, data}`
   envelope with independent major versions.
2. Emit named v1 envelopes from every CLI JSON command, including pubspec and
   pubspec-configuration commands.
3. Check in golden fixtures bound to the real file-analysis, project-analysis,
   URI-graph, GraphQL-contract, and Flutter-inventory models.
4. Document additive and breaking compatibility rules, deterministic ordering,
   and migration-history requirements.
5. Retain generic `to_json` and `to_json_pretty` helpers while explicitly documenting
   that raw Serde output is not a stable command-facing schema.

Verification:

- schema names are unique and every registered schema/version has a migration entry;
- golden changes fail focused tests until fixtures and compatibility documentation move;
- all seven CLI commands emit exactly `schema`, `version`, and `data` at the top level;
- exact Rust 1.95 formatting, Clippy, tests, rustdoc, edition, and feature checks pass;
- hosted Linux and Windows test and feature matrices report success.

Acceptance:

- every CLI JSON command emits a named versioned envelope;
- fixture ordering is deterministic on Windows and Linux;
- schema changes fail a focused test until the fixture and migration note are updated.

### DS-CLI-002: CLI Contract And Integration Tests

Status: verified. Priority: P1. Owner crate: `dartscope-cli`.

Implemented slices:

1. Global and command-specific help, package version output, and documented process exit
   codes for success, internal failure, invalid arguments, and unreadable inputs.
2. Process-level integration coverage for all seven JSON commands, invalid arguments,
   missing paths, repeatable environment pairs, and malformed source inputs.
3. Deterministic nested-package discovery with each package's nearest sibling
   `.dart_tool/package_config.json`.
4. Paths containing spaces, exact generated-directory exclusions, and a documented policy
   that recursive discovery never follows symlink entries.
5. Stable stdout/stderr separation: successful JSON commands emit one envelope to stdout,
   while argument and filesystem failures emit one human-readable error to stderr.

Verification:

- malformed Dart and YAML produce successful diagnostic-bearing JSON rather than panics;
- exact Rust 1.95 formatting and focused CLI tests pass;
- hosted Linux and Windows workspace tests and feature matrices report success;
- aggregate `dartscope/ci` reports success.

Acceptance:

- no command panics on malformed input;
- stdout contains JSON only for JSON commands and errors go to stderr;
- tests pass on Windows and Linux with paths containing spaces.

### DS-FLUTTER-002: Move Convention Extraction Behind Optional Boundary

Status: verified. Priority: P1. Prerequisites: DS-PARSE-005, DS-JSON-001.

Implemented slices:

1. Added parser-independent `DartInvocation`, argument, and map-entry facts with dotted targets,
   named and positional arguments, simple string values, result-member chains, enclosing callable
   IDs, exact invocation spans, and compatibility source-line spans.
2. The heuristic backend emits generic invocations after declaration inventory and reports the
   capability explicitly; declaration headers and masked non-code are excluded.
3. Removed Flutter route, asset, localization, and widget interpretation from `dartscope-parse`.
   Pure file/project analysis leaves the legacy Flutter projection empty and summary counts zero.
4. `dartscope-flutter` derives widget, `GoRoute`, `MaterialApp.routes`, asset, and localization
   findings from imports, declarations, invocations, and same-file constants. Older payloads that
   contain only legacy hints remain readable through a compatibility fallback.
5. Added explicit file/project composition APIs in the optional Flutter crate and
   `analyze_file_with_flutter` / `analyze_project_with_flutter` in the umbrella crate. The CLI uses
   those APIs for its v1 file/project payloads.
6. Documented the feature boundary and additive v1 migration without removing or renaming the
   legacy serialized field.

Verification:

- pure parser fixtures assert generic facts and an empty Flutter compatibility projection;
- optional-crate fixtures restore the existing 5 widgets, 3 routes, 2 assets, and 1 localization
  finding with deterministic paths, spans, confidence, resolution, and ordering;
- CLI smoke tests preserve file/project/inventory behavior while exposing generic invocations;
- exact Rust 1.95 workspace checks, tests, formatting, Clippy, rustdoc, minimal features, and all
  features pass locally before hosted finalization.

Acceptance:

- pure Dart parsing does not execute Flutter convention rules;
- disabling the `flutter` feature removes Flutter extraction code;
- existing Flutter fixtures retain findings, paths, spans, confidence, and ordering;
- `dartscope-index` remains independent from Flutter internals.

### DS-FLUTTER-003: Declared Assets And Localization Catalogs

Status: verified. Priority: P2. Prerequisites: DS-PUB-002, DS-FLUTTER-002.

Implemented slices:

1. Added explicit in-memory `FlutterL10nInput`, `FlutterArbInput`, and `FlutterCatalogInput`
   contracts; library analysis performs no filesystem I/O or Flutter process execution.
2. Linked direct literal `Image.asset`, `AssetImage`, and bundle-string uses to the nearest
   package pubspec. Exact file declarations and documented direct-child directory declarations are
   distinguished; package arguments naming external packages are not misreported as local misses,
   and non-literal package expressions remain explicit medium-confidence unresolved cases.
3. Preserved asset flavors and platforms in declaration links and reported high-confidence
   used-but-undeclared plus medium-confidence declared-but-unused diagnostics.
4. Parsed documented `l10n.yaml` paths, template file, output file, output class, output directory,
   and defaults. Duplicate, malformed, and missing-template states remain explicit.
5. Parsed ARB top-level message keys while excluding `@message` and `@@locale` metadata, linked
   generated localization getters and placeholder methods to catalogs, and diagnosed missing keys or unresolved output
   classes with confidence.
6. Extended `flutter-inventory` discovery to regular `l10n.yaml` and `.arb` files during the same
   deterministic, no-symlink project traversal. Added an additive v1 migration note and catalog
   boundary documentation.

Verification:

- focused fixtures cover exact files, direct directories, nested negative cases, unused
  declarations, external and dynamic package arguments, custom output classes, documented
  defaults, missing
  keys, and malformed YAML/JSON;
- CLI integration verifies catalog discovery and diagnostic JSON;
- older diagnostics deserialize without confidence and existing empty v1 golden output remains
  unchanged because new fields are optional and omitted when empty;
- exact Rust 1.95 workspace, feature, formatting, Clippy, rustdoc, and Linux/Windows checks pass.

Acceptance:

- library catalog analysis takes explicit source inputs and performs no I/O;
- asset links are package-aware and preserve declaration evidence;
- directory declarations do not recursively claim nested files;
- ARB metadata is not emitted as message keys;
- diagnostics carry deterministic paths, codes, spans when available, and confidence;
- malformed supplemental inputs never panic.

See `docs/development/flutter-catalogs.md`.

### DS-FLUTTER-004: Routes, Themes, And State Conventions

Status: verified. Priority: P2. Prerequisite: DS-FLUTTER-002.

Required work:

1. Add official `MaterialApp`, `WidgetsApp`, and `Navigator` route/navigation patterns before
   ecosystem-specific conventions.
2. Normalize supported theme construction and application facts without attempting full widget
   evaluation.
3. Define a versioned, opt-in support table for `go_router` and selected state-management
   packages, including package/version evidence and nearby negative fixtures.
4. Keep package conventions in `dartscope-flutter`; do not reinterpret application-specific
   manifests or move framework semantics into the pure parser.

Progress (2026-07-17):

- [x] Derive official `MaterialApp` and `WidgetsApp` home/default routes, route tables, and
  `initialRoute` facts, plus `Navigator.initialRoute`.
- [x] Derive official static and `Navigator.of(context)` named-route navigation calls, including
  restorable variants, with constant resolution and source spans.
- [x] Normalize official `ThemeData` construction and `MaterialApp`/`Theme`/`AnimatedTheme`
  application facts through a deterministic parser-independent API with source spans.
- [x] Define versioned opt-in `go_router`, `provider`, `flutter_riverpod`, and `flutter_bloc`
  support metadata, package/version evidence, activation statuses, and positive/negative fixtures.

Acceptance:

- official framework patterns have normative fixtures and source spans;
- ecosystem conventions expose support-version metadata and confidence;
- disabling the Flutter feature removes all new convention code;
- existing inventory ordering and JSON compatibility remain stable.

See `docs/development/flutter-ecosystem-conventions.md`.

### DS-INDEX-004: General Symbol And Namespace Resolution

Status: verified. Priority: P2. Prerequisite: DS-PARSE-006.

Generalize the proven import/export/part visibility machinery beyond GraphQL constants.
Resolve declarations with prefixes, combinators, privacy, re-exports, parts, and
conditional environments. Preserve ambiguous candidates and evidence.

Progress (2026-07-17):

- [x] Add a deterministic query API for top-level declarations with same-file, same-library,
  direct-import, re-export, prefixed-import, combinator, privacy, part, and conditional-environment
  behavior plus explicit candidate evidence.
- [x] Move GraphQL constant visibility onto the shared namespace engine without changing its public
  contract or fixtures; conditional-environment characterization now covers both public symbol queries
  and GraphQL operation linking.
- [x] Add opt-in conservative invocation-target reference facts with exact spans and confidence,
  plus deterministic batch resolution that reuses one namespace context without raw source parsing
  in the index crate.

See `docs/development/symbol-resolution.md`.

### DS-LINT-001: Optional Rule Engine

Status: verified. Priority: P3. Prerequisite: DS-INDEX-004.

Implemented (2026-07-17):

1. Added optional `dartscope-lints` with stable serialized rule IDs, explicit enablement,
   severity overrides, deterministic execution, summaries, spans, and related-path evidence.
2. Added configured exact/prefix forbidden-import rules and resolved internal layer-boundary rules.
3. Added conservative file/top-level declaration naming rules over normalized paths and declarations.
4. Added unresolved-part diagnostics backed by `dartscope-index` part-link outcomes.
5. Added explicit-entry-point orphan detection over resolved import/export/part reachability.
6. Kept all rules source-free and filesystem-free; existing seven CLI JSON v1 contracts are unchanged.

Verification:

- focused fixtures cover disabled defaults, severity overrides, forbidden imports, resolved layer
  targets, naming, unresolved parts, explicit orphan roots, deterministic ordering, and stable IDs;
- exact Rust 1.95 formatting, focused tests, workspace Clippy, rustdoc, workspace tests, and umbrella
  feature composition pass before finalization.

See `docs/development/lint-rules.md`.

### DS-RELEASE-001: Publishable 0.1 Release

Status: planned. Priority: P3. Prerequisites: DS-JSON-001, DS-CLI-002.

Add complete package metadata, rustdoc coverage, changelog, security policy, crate
publish order, `cargo package` checks, release CI, and an explicit support matrix for
Rust, Dart, Flutter, and ecosystem conventions.

### DS-COMPAT-001: Upstream Compatibility Radar

Status: research. Priority: P3. Not on the current 0.1 critical path.

Research scope:

1. Evaluate a CI-only official Dart analyzer oracle that compares DartScope's declared
   capabilities rather than requiring internal AST equality.
2. Define a version matrix where current stable can block changes, beta reports early drift,
   and dev/main channels are non-blocking compatibility radar.
3. Design a reduced conformance corpus, differential reports, scheduled release detection,
   and GitHub issue or draft-PR creation.
4. Define automation safety boundaries: CI may detect, minimize, report, and propose changes,
   but must not rewrite semantic parser rules, accept goldens, weaken assertions, or merge fixes.
5. Evaluate runtime cost, cache strategy, network and token permissions, upstream API stability,
   and the security boundary of any `tools/dart-oracle` prototype.

Research exit:

- official source and behavioral reference map is recorded;
- a bounded `tools/dart-oracle` exchange contract is prototyped or rejected with evidence;
- stable, beta, and development-channel failure policies are documented;
- implementation is split into reviewable follow-up tasks with explicit acceptance criteria.

## Completed Tasks

### DS-BOOT-001: Workspace Bootstrap

Status: verified.

Nine crates, MIT license, root README, fixtures, formatting, tests, clippy, lockfile,
Linux quality CI, Linux/Windows test CI, contribution guide, agent entrypoint, and
reference strategy exist. The repository builds independently and has no Athanor
dependency. The workspace MSRV is Rust 1.95, the exact Rust 1.95.0 toolchain is pinned,
and the virtual workspace uses edition 2024 with resolver 3. Hosted quality,
Linux/Windows tests, edition-2024, and umbrella feature checks pass and publish granular
plus aggregate commit statuses.

### DS-PARSE-001: File Analysis MVP

Status: implemented.

Imports, exports, conditional URIs, parts, part-of, library directives, declarations,
generic invocations, string constants, GraphQL documents/uses, spans, and diagnostics exist.
The pure parser does not execute Flutter convention rules.

### DS-PARSE-002: Cross-Platform Span And Diagnostic Attribution

Status: verified.

LF and CRLF byte starts are derived from original source segments. File, pubspec, and
package-config diagnostics carry normalized paths. Regression tests cover CRLF GraphQL,
part directives, and flattened project diagnostics.

### DS-PARSE-003: Modern Top-Level Type Forms

Status: verified.

The heuristic backend distinguishes modified classes, mixin classes, base mixins,
named and unnamed extensions, extension types, and prefixed Flutter base classes.

### DS-INDEX-001: URI Graph And Part Links

Status: verified for current model.

Relative, package, SDK, conditional, and unsupported URI outcomes are explicit and
sorted. Part ownership distinguishes matched, missing target, unresolved target,
missing part-of, and different library.

### DS-INDEX-002: GraphQL Contract Linking

Status: verified for current model.

Same-file, same-library, direct-import, and re-export visibility are supported with
combinators, privacy, cycles, conditional environments, call compatibility, and
variable compatibility.


### DS-JSON-001: Versioned JSON Contracts

Status: verified.

Seven CLI command families emit named v1 envelopes. Five public analysis families have
checked-in model-backed golden fixtures, raw Serde helpers remain explicitly unstable,
and compatibility plus migration rules are enforced by focused tests on Linux and Windows.


### DS-CLI-002: CLI Contract And Integration Tests

Status: verified.

All seven command families have stable help, version, exit-code, stdout/stderr, malformed
input, environment option, filesystem discovery, paths-with-spaces, generated-directory,
and symlink behavior covered by process-level tests on Linux and Windows.

### DS-PARSE-006: Complete Declaration Inventory

Status: verified.

Supported declarations now include type members and callable-local variables with stable
hierarchical IDs, parent relationships, and complete optional declaration spans. Traditional
constructors are distinguished from calls; unsupported Dart 3.13 constructor forms emit
explicit diagnostics rather than fabricated symbols.

### DS-FLUTTER-001: Inventory Aggregation

Status: verified for current input model.

The optional crate aggregates and deterministically sorts widgets, routes, assets,
localizations, and Flutter-related files. Route output preserves literal/expression
kind, resolved path, confidence, and source span.


### DS-FLUTTER-002: Optional Convention Boundary

Status: verified.

Pure parsing now emits generic invocation facts and no Flutter findings. The optional Flutter crate
owns convention interpretation, supports older legacy-hint payloads, and provides explicit file and
project composition APIs. CLI v1 behavior remains compatible through explicit composition.

### DS-FLUTTER-003: Asset And Localization Catalogs

Status: verified.

Direct literal asset uses now link to package pubspec declarations with official file/directory
semantics. Explicit `l10n.yaml` and ARB inputs produce generated-class/key links and
confidence-bearing catalog diagnostics without filesystem or Flutter process execution.

## Calibration Protocol

Use a real Flutter repository only after focused tests pass:

```powershell
cargo run -p dartscope-cli -- analyze-project D:\path\to\flutter_project
cargo run -p dartscope-cli -- uri-graph D:\path\to\flutter_project
cargo run -p dartscope-cli -- graphql-contracts D:\path\to\flutter_project
cargo run -p dartscope-cli -- flutter-inventory D:\path\to\flutter_project
```

For each discrepancy, record:

1. expected behavior and source class;
2. actual JSON finding or miss;
3. whether the problem is parser, resolver, index, Flutter convention, or consumer
   mapping;
4. the reduced synthetic fixture added to DartScope;
5. the verification command that proves the correction.

Do not paste calibration counts into this plan as permanent truth. Counts drift with
the external repository. Keep only reusable behavior and reduced fixtures here.

## Definition Of Done

A task is complete only when all applicable items pass:

- focused positive and negative tests exist;
- paths and spans are asserted for new findings;
- heuristic confidence or diagnostics are asserted;
- output ordering is deterministic;
- public Rust and JSON changes are documented;
- `README.md`, this plan, and `docs/reference-strategy.md` are synchronized;
- no Athanor or Rustok-specific domain logic was added;
- no unrelated working-tree changes were reverted;
- the following commands pass from `D:\DartScope` using the pinned Rust 1.95.0 toolchain:

```powershell
rustc --version
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

For CLI changes, run at least one successful command and one expected failure. For
package or release changes, run `cargo package` for the affected crate without
publishing.

## Stop And Escalate Conditions

Stop the current task and report the blocker when:

- official sources conflict or do not define the intended behavior;
- the change requires a breaking public-model or JSON migration not named by the task;
- a parser backend dependency raises an unresolved license, Rust 1.95 compatibility,
  maintenance, or process-execution concern;
- a real-project case cannot be reduced without exposing private source;
- the same finding requires consumer-specific semantics to become meaningful;
- unrelated user changes overlap the same code and cannot be preserved.

Do not resolve these conditions by silently expanding scope.

## Release Milestones

| Milestone | Exit condition |
| --- | --- |
| M0 trustworthy bootstrap | verified on exact Rust 1.95.0 across hosted Linux/Windows quality, tests, edition, and feature checks |
| M1 dependable heuristic toolkit | DS-MAINT-001, DS-PARSE-004, DS-PARSE-005, DS-PUB-002, DS-RESOLVE-003 |
| M2 stable tool boundary | DS-JSON-001, DS-CLI-002, compatibility policy |
| M3 optional Flutter pipeline | DS-FLUTTER-002 and declared asset/localization slice |
| M4 semantic project model | complete declarations and general symbol resolution |
| M5 community release | lint engine baseline and DS-RELEASE-001 |

## Current Recommended Next Step

Implement `DS-FLUTTER-004` next. Asset and localization catalogs now have explicit, package-aware
contracts; the next ready slice expands official framework navigation/theme facts and adds a
versioned opt-in ledger for selected ecosystem conventions. `DS-COMPAT-001` remains recorded as
research and is intentionally deferred until the current implementation queue is complete.
