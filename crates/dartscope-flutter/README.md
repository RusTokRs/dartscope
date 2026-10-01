# dartscope-flutter

Optional Flutter convention layer on top of `dartscope-core` analyses.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- `extract_flutter_inventory` lists widgets, routes, assets, and localizations of a project;
  `extract_flutter_inventory_with_catalogs` adds `.arb` and `l10n.yaml` catalogs.
- `derive_flutter_file_hints` and `populate_flutter_project_analysis` attach per-file hints with a confidence.
  A direct subclass of a Flutter widget base class is `High`; a transitive subclass is `Medium` and names the
  class it inherits through (`inherited_via`).
- `analyze_flutter_ecosystem` reports the supported ecosystem conventions (go_router, Provider, Riverpod,
  BLoC) from package and source evidence; `derive_flutter_theme_facts` reports theme construction.
- It depends only on `dartscope-core`, never re-exports parser internals, and does not read files.

## Example

```rust,ignore
use dartscope_flutter::extract_flutter_inventory;

let inventory = extract_flutter_inventory(&project_analysis);
println!("{} widgets", inventory.widgets.len());
```

## Documentation

- [API reference](https://docs.rs/dartscope-flutter)
- [Flutter boundary](https://github.com/RusTokRs/dartscope/blob/main/docs/development/flutter-boundary.md)
- [Ecosystem conventions](https://github.com/RusTokRs/dartscope/blob/main/docs/development/flutter-ecosystem-conventions.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
