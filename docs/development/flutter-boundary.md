# Optional Flutter Convention Boundary

DartScope separates source parsing from Flutter convention interpretation.

## Pure parser contract

`dartscope-parse` emits normalized Dart facts only:

- imports and exports;
- declarations with stable ownership;
- generic invocations with dotted targets;
- positional and named arguments;
- simple string values and map entries;
- result-member chains, enclosing callable IDs, and source spans.

It does not decide that a class is a Flutter widget, that a `GoRoute` call is a route, or that an
`Image.asset` argument is an asset reference. Pure `analyze_file` and `analyze_project` therefore
leave `DartFileAnalysis.flutter` empty and report zero Flutter summary counts.

## Optional convention APIs

`dartscope-flutter` interprets generic facts through explicit APIs:

- `derive_flutter_file_hints` returns a derived file projection without mutating the input;
- `populate_flutter_file_hints` writes the v1 compatibility projection for one file;
- `populate_flutter_project_analysis` writes all file projections and recomputes project summary
  counts;
- `extract_flutter_inventory` derives a sorted inventory directly from either pure generic facts
  or an older payload containing only legacy Flutter hints;
- `extract_flutter_inventory_with_catalogs` additionally links direct asset uses to pubspec
  declarations and validates generated-localization uses against explicit `l10n.yaml` and ARB
  inputs.

The umbrella crate exposes `analyze_file_with_flutter` and `analyze_project_with_flutter` when both
`parse` and `flutter` features are enabled. The CLI intentionally uses these composition APIs for
`analyze-file` and `analyze-project` so its v1 payload behavior remains compatible. Its
`flutter-inventory` command also supplies discovered `l10n.yaml` and ARB text to the catalog API.
The library itself never discovers or reads those files, and non-catalog CLI commands do not
read them.

## Compatibility policy

`DartFileAnalysis.invocations` is an additive optional v1 field. The legacy `flutter` field remains
serialized and readable. New pure-parser consumers should treat invocations as the source facts and
request Flutter derivation explicitly. Older payloads without invocations remain supported by the
Flutter inventory fallback.

Removing or renaming the legacy projection requires a future JSON major version and migration
fixtures; this task does not perform that breaking change.

## Feature boundary

Disabling the umbrella `flutter` feature removes `dartscope-flutter` and all convention extraction
code. `dartscope-index` consumes generic analysis only and remains independent from Flutter
internals. No normal analysis path invokes `dart` or `flutter` processes.

## Catalog boundary

Asset and localization catalogs remain optional Flutter-layer analysis. `dartscope-core` owns the
shared diagnostic and compatibility types, while YAML/JSON parsing and Flutter semantics stay in
`dartscope-flutter`. See [`flutter-catalogs.md`](flutter-catalogs.md).
