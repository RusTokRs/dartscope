---
id: doc://docs/development/dartscope-library-plan.md
kind: development_plan
language: en
source_language: en
status: draft
---

# DartScope Library Plan

DartScope is a standalone Rust toolkit for Dart and Flutter code intelligence.
It is not an Athanor adapter and must not depend on Athanor crates or Athanor
domain types.

The repository lives at `D:\DartScope` and should be developed as a community-facing
library ecosystem that can be used by CLI tools, CI checks, editors, code review bots,
migration tools, graph exporters, and downstream products.

## Purpose

DartScope should expose reusable APIs for:

- Dart source parsing and source spans
- declarations, imports, exports, parts, and part-of discovery
- Dart-embedded GraphQL operation discovery
- package metadata and dependency analysis
- project and package indexing
- symbol and import/export resolution where feasible
- Flutter-aware inventory and convention extraction
- optional lint/rule engines over the normalized model
- optional macro-aware analysis
- stable JSON output for external tools

DartScope output is a DartScope-owned analysis model. It should not emit any
consumer-specific graph model as its primary API.

## Product Shape

DartScope should be a modular Rust workspace with narrow crates for focused community
use and one wide umbrella crate for full-pipeline consumers.

Recommended crate layout:

```text
D:\DartScope\
  Cargo.toml
  README.md
  LICENSE
  docs\
    development\
      dartscope-library-plan.md
  crates\
    dartscope\
    dartscope-core\
    dartscope-parse\
    dartscope-resolve\
    dartscope-index\
    dartscope-flutter\
    dartscope-lints\
    dartscope-json\
    dartscope-cli\
    dartscope-macros\
```

Crate responsibilities:

```text
dartscope-core      domain model, spans, symbols, diagnostics, ports
dartscope-parse     parser facade and parser backend adapters
dartscope-resolve   imports, exports, packages, parts, symbol resolution
dartscope-index     project/package index and cross-file lookup
dartscope-flutter   Flutter-specific conventions and framework hints
dartscope-lints     optional lint/rule engine over the normalized model
dartscope-macros    optional future macro expansion or macro-aware analysis
dartscope-json      stable JSON serialization boundary for tools
dartscope-cli       command line wrapper for community workflows
dartscope           thin umbrella crate with feature-gated re-exports
```

Focused community use:

```toml
[dependencies]
dartscope-parse = "0.x"
```

Full-pipeline use:

```toml
[dependencies]
dartscope = { version = "0.x", features = ["parse", "resolve", "index", "flutter", "json"] }
```

## Architecture

DartScope should use ports-and-adapters architecture.

Core boundaries:

- `dartscope-core` owns stable DartScope types and traits only.
- Parser backends implement a `ParserPort` or equivalent trait outside core.
- Package and symbol resolution consume normalized parse output and should not depend
  on a specific parser backend.
- Linters consume the normalized project model and emit DartScope diagnostics; they
  should not parse source files directly.
- Macro support should be optional and isolated behind a macro expansion or
  macro-aware analysis port.
- Flutter conventions should live outside pure Dart core so non-Flutter users can
  avoid Flutter-specific dependencies.
- JSON and CLI output are adapters around the DartScope model, not the primary
  internal representation.
- The umbrella `dartscope` crate should stay thin and should not become a second
  implementation location.

This split keeps parser backends, lint rules, macro handling, Flutter heuristics,
serialization, and CLI workflows replaceable.

## Reference Strategy

DartScope must be built from documented language and framework behavior, not from
guesses. The living source map is `docs/reference-strategy.md`.

Source classes:

```text
normative      official specifications, official docs, official API docs
behavioral     Dart analyzer, dart/pub/flutter tool behavior
implementation parser crates, analyzer bridges, tree-sitter grammars
ecosystem      community tools and conventions
consumer       downstream integration needs such as Athanor
```

Use normative and behavioral sources to define semantics. Use implementation and
ecosystem sources as references for practical coverage, not as sources of truth.

Initial required references:

- Dart language specification
- official Dart language tour and feature docs
- official docs for libraries, imports, exports, parts, and packages
- official `pubspec.yaml` and package layout docs
- official Dart analyzer behavior where command output is relevant
- official Flutter API docs for core widgets and navigation primitives
- official Flutter docs for assets, localization, routing, themes, and widget structure
- official package docs for widely used routing packages only when DartScope explicitly
  supports those package conventions

Implementation references can include parser crates, tree-sitter grammars, analyzer
bridges, `custom_lint`, `build_runner`, `melos`, and other ecosystem tools, but each
adopted behavior should be traceable to a documented source or explicitly marked as a
heuristic.

## Core Model

DartScope should expose stable analysis types rather than parser-specific AST nodes as
the main public API.

Candidate top-level API:

```rust
pub fn analyze_file(input: DartFileInput) -> DartFileAnalysis;
pub fn analyze_project(input: DartProjectInput) -> DartProjectAnalysis;
pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis;
pub fn build_import_graph(project: &DartProjectAnalysis) -> ImportGraph;
pub fn extract_flutter_inventory(project: &DartProjectAnalysis) -> FlutterInventory;
```

Candidate file analysis output:

```rust
pub struct DartFileAnalysis {
    pub path: String,
    pub language: DartFileLanguage,
    pub imports: Vec<DartImport>,
    pub exports: Vec<DartExport>,
    pub parts: Vec<DartPart>,
    pub part_of: Option<DartPartOf>,
    pub declarations: Vec<DartDeclaration>,
    pub flutter: FlutterFileHints,
    pub diagnostics: Vec<DartDiagnostic>,
}
```

Initial declaration kinds:

- class
- mixin
- enum
- extension
- typedef
- top-level function
- method
- constructor
- field
- variable

Diagnostics should represent uncertainty explicitly. The library should not silently
pretend that a heuristic result is complete.

## Parser Strategy

The public model should stay backend-independent.

Implementation stages:

1. Start with a conservative Rust implementation for high-confidence declarations,
   imports, exports, parts, and simple Flutter patterns.
2. Add a parser backend abstraction before the first public API hardens.
3. Evaluate `tree-sitter-dart`, native Rust parsers, or an official analyzer bridge as
   optional backends for deeper coverage.
4. Keep any official analyzer bridge optional because it likely runs an external Dart
   process and has a different deployment shape than a pure Rust library.

The public positioning should remain `DartScope: a Dart and Flutter code intelligence
toolkit`, not "a lightweight parser".

## Community Use Cases

DartScope should support:

- Rust tools that need to parse or inspect Dart source
- package and import/export graph generation
- Flutter inventory tools for widgets, screens, routes, assets, localization, and themes
- architecture checks for layers, forbidden imports, naming, and package boundaries
- CI checks for orphan files, unresolved parts, missing route coverage, and deprecated
  API usage
- code review bots that summarize public API, dependency, route, widget, and screen
  changes
- migration and codemod tools that need a project index before rewriting code
- code generation for route manifests, package manifests, docs, widget catalogs, and
  test plans
- security and compliance scanners for platform channels, permissions, storage,
  analytics SDKs, logging, and possible secret handling
- editor, IDE, LSP, and analyzer-adjacent tooling
- stable JSON export for graph visualizers and other external tools

Visualization is not a primary DartScope responsibility. DartScope should provide
structured data that visualizers can consume.

## Development Phases

### Phase 1: Repository Bootstrap

Status: planned.

Scope:

- add a Rust workspace
- add a root README, license, contribution notes, and package metadata
- add initial fixtures for Dart libraries and Flutter apps
- set up formatting, tests, clippy, and CI
- document reference strategy and non-goals

Acceptance:

- repository builds independently
- README explains community use cases
- initial API is shaped around analysis results, not parser internals

### Phase 2: File Analysis MVP

Status: in progress.

Scope:

- parse imports, exports, parts, part-of declarations, and top-level declarations
- detect class inheritance and common Flutter base classes
- parse `pubspec.yaml` package metadata and dependencies
- expose a project-level CLI smoke command for real Flutter repositories
- expose line/column or byte-range spans for every finding
- report partial parse and unsupported syntax diagnostics

Acceptance:

- fixtures cover pure Dart, Flutter widgets, syntax errors, parts, exports, and package
  dependencies
- results are deterministic on Windows and Unix path spellings
- every finding has a source span suitable for downstream source attribution
- real-project misses are converted into small fixtures before broadening heuristics

Current calibration:

- `dartscope analyze-project D:\RusTok\rustok_mobile\apps\rustok_frontend_mobile`
  reports 8 Dart files, 1 pubspec, 34 imports, 66 declarations, 10 Flutter widget
  hints, 6 Flutter route hints, 6 GraphQL operations, 6 GraphQL operation uses, 8
  package dependencies, and 0 diagnostics after reducing false positives from indented
  Flutter constructor calls and top-level `const` initializers. Storefront route
  constants resolve to paths such as `/`, `/catalog`, `/cart`, `/checkout`, `/profile`,
  and `/modules/:routeSegment`.
- `dartscope analyze-project D:\RusTok\rustok_mobile` reports 69 Dart files, 10
  pubspecs, 172 imports, 26 exports, 229 declarations, 22 string constants, 12
  GraphQL operations, 12 GraphQL operation uses, 36 Flutter widget hints, 10 Flutter
  route hints, 30 package dependencies, and 0 diagnostics.
- GraphQL use calibration links operation constants to repository/client call sites:
  storefront catalog/cart queries and cart mutations map to their repository methods;
  modules queries and mutations map to `listModules`, `toggleModule`,
  `failedRecoveryPlans`, `retryFailedPostHook`, and `compensateFailedOperation`.
  The admin bootstrap query maps to the top-level `authBootstrapProbeProvider`
  variable initializer through `enclosing_symbol`. `gql(r'''...''')` inline documents
  are not reported as constant uses.
- GraphQL variable calibration extracts declared operation variables from Dart-embedded
  GraphQL documents and supplied top-level client variables from `QueryOptions` /
  `MutationOptions`. In `D:\RusTok\rustok_mobile`, declared and supplied variable
  names currently match for all 12 operation uses, including storefront cart mutations
  and modules recovery mutations.
- The calibration project is not copied into this repository. Reusable cases are
  reduced into fixtures, currently covering widget constructor calls, top-level
  variable initializers, `State`, Riverpod `ConsumerWidget`, and `go_router`
  `GoRoute` route hints with same-file string constant resolution, plus Dart raw
  string GraphQL operation documents, declared variables, and client operation
  constant uses with supplied variables.

### Phase 3: Project Index

Status: planned.

Scope:

- build a package-level import/export graph
- resolve project-relative Dart file references where straightforward
- connect `part` and `part of` files
- expose stable JSON export for CLI and downstream tooling
- preserve omitted or unresolved edges explicitly

Acceptance:

- graph output is deterministic and independent from any consumer-specific graph model
- unresolved imports and missing part files are diagnostics, not silent gaps
- monorepo-style package fixtures can be analyzed

### Phase 4: Flutter Conventions

Status: planned.

Scope:

- extract widgets, screens, common route declarations, route names, and route targets
- detect common state-management declarations as optional conventions, starting with
  high-confidence patterns only
- detect asset and localization usage where syntax is direct
- expose confidence or diagnostic metadata for heuristics

Acceptance:

- fixtures cover `MaterialApp.routes`, `Navigator`, and `GoRouter` style declarations
  where feasible
- uncertain dynamic routing is reported as uncertain output
- Flutter-specific output remains optional for pure Dart consumers

### Phase 5: Lints And Rules

Status: planned.

Scope:

- add an optional rule engine over the normalized project model
- support architecture rules, forbidden imports, naming conventions, and layer policies
- keep rules independent from parser backend internals

Acceptance:

- users can run rules without depending on Athanor or another downstream product
- diagnostics include source spans and rule ids

## Verification

Run in `D:\DartScope`:

```bash
cargo fmt --all
cargo test --workspace --quiet
cargo clippy --workspace --all-targets -- -D warnings
```

## Open Decisions

- exact first-release crate set
- license and contribution model
- initial parser backend
- initial Flutter package conventions to support beyond Flutter SDK APIs
- whether the CLI ships before or after the library API stabilizes

## Current Recommended Next Step

1. Add the root README and Rust workspace.
2. Add `dartscope-core`, `dartscope-parse`, `dartscope-json`, `dartscope-cli`, and
   umbrella `dartscope`.
3. Implement file-level imports/declarations and `pubspec.yaml` dependency parsing.
4. Add fixtures tied to the reference strategy.
