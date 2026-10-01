# dartscope-lints

Optional deterministic lint rules over normalized project and index facts.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- Rules: `dartscope.forbidden_import`, `dartscope.layer_boundary`, `dartscope.naming_convention`,
  `dartscope.unresolved_part`, and `dartscope.orphan_file`. Nothing runs unless it is enabled in an explicit
  `DartLintConfig`; severities can be overridden per rule, and generated files can be excluded.
- `lint_project` lints a project analysis, `lint_workspace_snapshot` a workspace snapshot, and
  `DartIncrementalLintCache` re-lints only the libraries an update affected, with the same result as a full run.
- No source parsing and no filesystem access.

## Documentation

- [API reference](https://docs.rs/dartscope-lints)
- [Lint rules](https://github.com/RusTokRs/dartscope/blob/main/docs/development/lint-rules.md)
- [Lint command](https://github.com/RusTokRs/dartscope/blob/main/docs/development/lint-cli.md)
- [Incremental lints](https://github.com/RusTokRs/dartscope/blob/main/docs/development/incremental-lints.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
