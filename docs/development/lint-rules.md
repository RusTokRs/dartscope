---
id: doc://docs/development/lint-rules.md
kind: development_note
language: en
source_language: en
status: active
---

# Optional Lint Rule Engine

`dartscope-lints` runs deterministic rules over normalized `DartProjectAnalysis` and
`dartscope-index` results. It does not read files, parse source text, or invoke Dart/Flutter tools.

## Public API

- `lint_project(&DartProjectAnalysis, &DartLintConfig) -> DartLintAnalysis`
- `lint_workspace_snapshot(&DartWorkspaceSnapshot, &DartLintConfig) -> DartLintAnalysis` reuses the
  snapshot's URI graph and part links.
- `DartIncrementalLintCache` retains local diagnostics by Dart library and consumes
  `DartWorkspaceUpdate::affected_libraries`.
- `DartLintRuleId::ALL` lists built-in rules in stable execution order.
- `DartLintConfig::default()` enables no rules.
- severity overrides use `DiagnosticSeverity`.
- diagnostics retain rule ID, severity, message, normalized path, optional source span, and optional
  related paths.

## Built-In Rule IDs

- `dartscope.forbidden_import` matches configured exact or prefix import URI patterns, optionally
  scoped to source path prefixes.
- `dartscope.layer_boundary` checks resolved internal import targets against configured source and
  denied target path prefixes.
- `dartscope.naming_convention` checks lower-snake-case Dart file-name segments and conservative
  ASCII casing for supported top-level declarations.
- `dartscope.unresolved_part` converts non-matched normalized part-link outcomes into lint
  diagnostics.
- `dartscope.orphan_file` computes reachability through resolved import/export/part edges from
  explicitly configured entry points.

Configuration order does not control execution order. Findings are sorted by normalized path, span,
rule ID, message, and related evidence, then deduplicated.

## Conservative Boundaries

Forbidden import, layer, and orphan behavior is inert until the caller supplies patterns or entry
points. Naming ignores non-ASCII identifiers rather than claiming Unicode casing completeness.
Orphan analysis returns no findings when none of the configured entry points exist in the loaded
project. External packages absent from the project index are not treated as internal layer targets.

The crate remains an optional umbrella feature. `dartscope lint` is a separate filesystem adapter
that maps versioned TOML into this API and emits `dartscope.lint-analysis` v1 or SARIF 2.1.0 without
moving rule semantics into the CLI crate. See `docs/development/lint-cli.md` and
`docs/development/incremental-lints.md`.
