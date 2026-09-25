---
id: doc://docs/development/audit-findings-2026-09-25.md
kind: development_note
language: en
source_language: en
status: active
---

# Repository Audit — Findings And Corrections (2026-09-25)

This note records the engineering review of the DartScope workspace requested for the current cycle:
every defect, unfinished slice, and architectural gap that was found, what was changed, how it was
verified, and what deliberately remains open. Commits referenced here are on the review branch that
started from `b9134c6` ("Bound CLI project traversal (#149)").

## Verification Summary

| Check | Result |
| --- | --- |
| `cargo check --workspace --all-targets` | pass (no workspace warnings) |
| `cargo test --workspace` | **384 passed, 0 failed, 0 ignored** (baseline `b9134c6`: 333) |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (only third-party `lazy_static`/`winnow`/`toml` warnings from the vendored dependency set) |
| `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` | pass |
| `cargo check -p dartscope --no-default-features` / `--all-features` | pass |
| `python3 -m unittest discover -s tools/tests` | 22 passed |
| `tools/check-repository-consistency.py`, `check-workflow-policy.py`, `check-dependency-policy.py` | pass |
| CLI smoke: success path, expected failure, `lint` findings/config exit codes | pass |
| Corpus differential check: `dart-lang/http`, `felangel/bloc`, `dart-lang/shelf` | 0 missing / 0 extra type declarations after finding 9 |

Local verification uses the pinned workspace sources with a locally assembled Rust 1.88 toolchain and
path-patched dependency checkouts, because this environment cannot reach `static.rust-lang.org`,
`crates.io`, or GitHub release assets. Rust version, edition, and resolver therefore match the
repository only by source, not by compiler patch level; hosted Linux/Windows/macOS gates remain the
authority for the exact Rust 1.95.0 toolchain.

## Findings And Corrections

### 1. Declaration inventory was line-anchored (dropped declarations) — fixed

`crates/dartscope-parse/src/declaration_inventory` classified at most one declaration per source line
and only if it started that line. Everything below was invisible to `analysis.declarations`, to the
index that consumes it, and to every consumer of member navigation:

- one-line type bodies: `class A { final int count = 0; }`;
- second member on a line: `final int count = 0; int run() { ... }`;
- type body opening on the header line: `class A { int run() { ... } }`;
- local variables declared on the callable's first line: `void f() { var a = 1; }`;
- explicitly typed, `late`, and `late final` top-level variables: `int counter = 0;`,
  `late String title = 'x';`, `late final int total = 1;`, `int first, second;`;
- annotated members and locals sharing a line with their annotation: `@override int get x => 1;`,
  `@Deprecated('l') final int local = 1;`;
- declarations after masked comments and after a directive on the same line.

The scanner is now byte-anchored: it walks each line, computes the brace depth *at the candidate
position*, scans multiple declarations per line, and advances to the next code byte after each parsed
declaration. Positive fixtures live in
`crates/dartscope-parse/tests/declaration_inventory_layout.rs`.

Correcting this also required removing the accidentally-load-bearing behaviour that the old line
anchor provided, so the following negatives were added and are covered by fixtures:

- a multi-line initializer whose header ends at a `=>` token
  (`final GoRouter appRouter = GoRouter(... builder: (context, state) => ...`) must still yield the
  variable and must never fabricate a `Function GoRoute` from a continuation line;
- the `span` field stays the declaration's source-line anchor (column 1), while `declaration_span`
  carries the exact byte/line/column range;
- continuation lines keep their indentation guard, so a multi-line list or call argument is not read as
  a declaration.

### 2. Metadata annotations hid their declaration — fixed

A leading `@` made the header unparseable, so the whole declaration was skipped. An annotation-aware
scanner now consumes `@name`, dotted names, optional type arguments, and balanced argument lists
(including arguments that span lines), and also advances past an annotation's closing `)` when the
declaration shares that line. Fixtures:
`crates/dartscope-parse/tests/declaration_inventory_annotations.rs`.

### 3. Normal `factory` constructors were misdiagnosed and dropped — fixed

`is_concise_constructor` matched any header beginning with `factory`, so ordinary Dart such as

```dart
factory A.fromJson(int value) => A();
const factory A.aliased() = B;
```

emitted a fabricated `unsupported_concise_constructor` warning and was removed from the inventory
entirely. The guard is now owner-aware: only the unprefixed Dart 3.13 concise forms (`new(...)`,
`factory <unqualified-name>(...)`) are diagnosed, `factory Owner()`/`factory Owner.name()` are collected
as constructors, and a declaration that follows a concise constructor on the same line is still
collected. Fixtures: `crates/dartscope-parse/tests/declaration_inventory_constructors.rs`.

### 4. String constants and directive URIs were truncated — fixed

`DartStringConstant.value` was produced by "find the first quote and take text up to the next one",
which broke on:

- triple-quoted and raw literals: `const q = r'''...'''` reported an empty value, including the common
  multi-line GraphQL document constants;
- escaped quotes: `'it\'s'` reported `it\`;
- adjacent literal concatenation: `'/modules' '/:id'` reported only `/modules`;
- multi-line literals, whose span covered only the first line.

The same helper backs `import`/`export`/`part`/`part of` URIs, so those were truncated the same way.
String literals are now read through the scanner that lexical masking already uses
(`string_literal_range`/`string_literals_value` in `dartscope-parse/src/lexical.rs`), which handles raw
strings, triple quotes, escapes, and concatenation. Literal content is reported as written (no
unescaping) and the span is the exact literal range, including multi-line raw strings. Fixtures:
`crates/dartscope-parse/tests/string_constants.rs`.

A second defect in the same path is worth calling out: any `const`/`final` whose initializer merely
*contained* a literal — `final value = readString('key');` — was reported as a string constant. Only a
literal initializer is now reported.

### 5. Unqualified same-owner member navigation (DS-INDEX-006 ordered slice) — implemented

The repository's own "Current Next Step" asked for unqualified `method()` calls, property reads, and
property writes inside a callable with one exact enclosing type. Implemented as
`crates/dartscope-parse/src/unqualified_member_references.rs` plus emitter changes in
`identifier_references.rs`, `lexical_reads.rs`, and `lexical_writes.rs`:

- a member fact is produced only when the enclosing callable supplies one exact owner symbol ID and that
  owner directly declares a matching method, field, getter, or setter;
- `this`/`super` roots stay excluded, and visible parameters, block-locals, import prefixes,
  enclosing-owner members, and declaration-shaped local functions suppress the heuristic;
- static-versus-instance evidence is explicit, and compound assignment/increment targets emit the paired
  read-then-write facts instead of one fabricated target;
- no new public reference kind or serialized field was added.

Index-side resolution reuses the directly declared exact-owner member inventory, keeps the missing-owner
fallback, private-library visibility, validated parts, reverse references, and full-build versus
snapshot parity. Fixtures: `crates/dartscope-parse/tests/unqualified_member_references.rs` (4 tests) and
`crates/dartscope-index/tests/navigation_unqualified_members.rs` (2 tests).

### 6. Pubspec dependency source round trip was lossy — fixed

The flattened `version_or_source` compatibility string was not a lossless projection of the typed
`PubspecDependencySource`. A URL containing the `;` field separator, or the scalar `git:`/`hosted:`
shorthand with a sibling `version:`, rebuilt a different source when a consumer echoed the string back.
`to_normalized_source`, `from_flattened_fields`, and the parse-side splitter now share one escaping
contract, and the constructor's debug assertion compares against the rendered projection. Fixtures:
`crates/dartscope-parse/tests/pubspec_dependency_sources.rs` (5 tests).

### 7. Incremental index counters and CLI traversal determinism — fixed

- `per_file_caches_rebuild_only_relevant_sources` asserted a `reference_files_rebuilt` delta of `+1`.
  Instrumenting the rebuild plan showed the change is legitimate: the edited file now contains a
  directly declared member, so it becomes a reference-bearing source and must be re-resolved in
  addition to its dependent importer — delta `+2`. The expectation and its comment were corrected
  instead of weakening the test.
- `dartscope-cli` collected directory entries in filesystem order before descending, so *which* entry
  triggered a traversal-limit diagnostic depended on the host. Entries are now sorted per directory,
  and a fixture asserts that the reported entry is the sorted-first one.

### 8. Dead code and repository hygiene — fixed

- Removed `pubspec_yaml_marked::into_diagnostics` and both unused `#[allow(dead_code)]` attributes in
  `dartscope-parse/src/lib.rs`; `crates/*/src` now contains no `dead_code` allow.
- Removed the unused `_path` parameter from `collect_locals`.
- Deleted the stray `flutter004-test.log` from the repository root.

### 9. `$` inside an identifier truncated every name that contains it — fixed

Every scanner carried its own `is_identifier_start`/`is_identifier_continue` character class built from
`is_ascii_alphanumeric() || '_'`, so the Dart identifier rule was wrong everywhere at once: `$` is a
valid `IDENTIFIER_START` and `IDENTIFIER_PART`, and generated or framework-facing sources rely on it
(`_$UserFromJson`, `UrlRequestCallbackProxy$Interface`, `jni$_`).

The defect was found by a differential check of the analyzer against real code rather than by unit
tests: `analyze-project` was run over depth-1 clones of `dart-lang/http`, `felangel/bloc`, and
`dart-lang/shelf` (338, 616, and 99 Dart files) and the declarations in the JSON output were compared
with the declarations a masked-source regex finds. Type declarations matched 1:1, but the affected
files showed the truncation directly:

- `pkgs/cronet_http/lib/src/jni/jni_bindings.dart`: `final class _$UrlRequestCallbackProxy$UrlRequestCallbackInterface`
  was reported as `class '_'`, `$DnsOptions$Experimental` as `class 'DnsOptions'`, and
  `extension type UrlRequestCallbackProxy$UrlRequestCallbackInterface` as
  `extension_type 'UrlRequestCallbackProxy'`.
- The same truncation hit members (`count$` reported as `count`), import prefixes (`as bindings$`),
  `show`/`hide` combinators, type annotations, and GraphQL constant names such as `query$`.

The fix is one canonical module instead of thirteen copies:
`crates/dartscope-parse/src/identifiers.rs` defines `is_identifier_start`, `is_identifier_continue`,
`identifier_end`, `leading_identifier`, and `is_identifier`, and declaration name extraction,
`declaration_inventory/syntax.rs`, the reference scanners, lexical binding and read/write scanners,
member/property/operator scans, `namespace.rs`, and `invocations/` all use it. Consequences of the
shared rule:

- `next_identifier`/`next_qualified_identifier` no longer truncate, and the naming-convention rule
  strips leading `_`/`$` decoration and accepts `$` inside a name, so `Widget$Base` and `count$` are no
  longer reported as case violations.
- `invocations/scanner.rs::is_chain_start` no longer needs its special `$` guard, because `$` is an
  identifier byte.
- GraphQL keeps its own grammar: `Name ::= /[_A-Za-z][_0-9A-Za-z]*/` has no `$`, so
  `graphql.rs::next_graphql_name` deliberately stays dollar-free.

Fixtures: `declaration_inventory_dollar_names.rs` (6), `dollar_identifier_references.rs` (3),
`dollar_identifier_navigation.rs` (2), plus two naming-rule tests in `dartscope-lints`.

### 10. Unnamed extensions were dropped together with their whole body — fixed

`extension on List<int> { ... }` has no declarable name, and `extension_declaration_name` returned
`None` for it. The declaration was therefore invisible *and* the inventory never scanned its body, so
every member was lost. The corpus check surfaced it in
`pkgs/ok_http/lib/src/ok_http_web_socket.dart` and
`pkgs/shelf_router/benchmark/router_benchmark.dart`.

Such an extension is now returned with an empty name, a stable `<path>::extension:` symbol ID
(`#2`, `#3`, ... for repeats), and its members keep it as parent. The naming-convention rule accepts an
empty name, because an anonymous declaration has no case convention to violate.

## Deliberately Open Or Out Of Scope

These were reviewed and left alone; each is recorded so the next cycle does not re-discover it as a
"defect":

- **Local functions are not lexical bindings.** The unqualified-member guard scans the masked callable
  body for a declaration-shaped local function and only ever *suppresses* member evidence. Modeling
  local functions as bindings is its own evidence-gated slice (a declaration after a use currently
  suppresses a legitimate member fact conservatively).
- **Wildcard declarations.** `var _ = 1;` yields a `local_variable` declaration with a stable symbol ID
  (`.../local_variable:_`, `#2`, ... for repeats). Namespace and lexical resolution already exclude
  wildcards, so no reference resolves to it; hiding the declaration would change the public inventory
  contract.
- **Enum constants are not modeled.** `DartDeclarationKind` has no enum-constant variant, so enum member
  scanning starts after the constant list's `;`. Adding a public kind needs a compatibility decision.
- **Top-level `get`/`set` accessors are not inventoried.** The task scope is declaration bodies; an
  accessor is neither a variable nor a function, and the inventory no longer fabricates one.
- **Non-ASCII identifiers.** The canonical scanner is byte-based and ASCII-only, like every other
  scanner in the conservative backend; Dart permits non-ASCII letters, which stays out of scope until
  the backend gains a real lexer.
- **Inherited members, extension selection, implicit constructors, receiver type inference, cascades,
  null-aware access, and flow-sensitive behaviour** remain behind later `DS-INDEX-006` slices, as the
  plan states.
- **`cargo package --locked` cannot be exercised here.** Cargo rejects packaging with the path-patched
  vendored dependency set, and the sandbox cannot reach `crates.io`; archive validation remains a hosted
  CI gate. Likewise `cargo test --workspace --locked` is meaningful only in the repository checkout,
  because the offline build copy rewrites `Cargo.lock` for the patched dependency set.
- **Exact-toolchain and platform gates.** macOS arm64, Windows, fuzzing, RustSec, `cargo-machete`, and
  the benchmark signal are hosted-only; nothing in this review changed their configuration.

## Changed Paths

Production: `dartscope-parse` (`identifiers.rs` (new), `declaration_inventory/{mod,scanner,syntax}.rs`,
`analysis.rs`, `declarations.rs`, `graphql.rs`, `lexical.rs`, `namespace.rs`,
`identifier_references{.rs,/{typed,typed_positions}.rs}`, `invocations/{scanner,arguments}.rs`,
`lexical_bindings.rs`, `lexical_reads.rs`, `lexical_regions/scan.rs`, `lexical_writes.rs`,
`member_reference_syntax.rs`, `operator_references.rs`, `property_references.rs`,
`unqualified_member_references.rs`, `lib.rs`, `pubspec_yaml_marked*.rs`),
`dartscope-core` (`pubspec.rs`), `dartscope-index` (`navigation/members.rs`, `src/tests/incremental.rs`,
`tests/navigation_properties.rs`), `dartscope-lints` (`rules/naming.rs`), `dartscope-cli` (`main.rs`,
new traversal-order fixture).

Tests added: `declaration_inventory_layout.rs` (8), `declaration_inventory_constructors.rs` (5),
`declaration_inventory_annotations.rs` (5), `declaration_inventory_dollar_names.rs` (6),
`string_constants.rs` (6), `dollar_identifier_references.rs` (3),
`unqualified_member_references.rs` (4), `navigation_unqualified_members.rs` (2),
`dollar_identifier_navigation.rs` (2), `pubspec_dependency_sources.rs` (5), two `dartscope-lints`
naming-rule tests, plus one CLI traversal fixture.

Documentation: `docs/development/dartscope-library-plan.md` (DS-PARSE-002, DS-PARSE-006, DS-PUB-002,
DS-INDEX-006 progress entries and "Current Recommended Next Step"), `AGENTS.md` (Current Next Step),
`docs/development/ds-index-006-progress-2026-07-22.md`, and `CHANGELOG.md`.
