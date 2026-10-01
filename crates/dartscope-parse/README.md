# dartscope-parse

Conservative, source-only analysis of Dart files, Dart projects, and `pubspec.yaml`.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- `analyze_file` and `analyze_project` report imports, exports, parts, declarations, invocations, string
  constants, and Dart-embedded GraphQL with exact spans and diagnostics. `analyze_file_with_references` and
  `analyze_project_with_references` add identifier references and lexical bindings.
- `parse_pubspec` and `parse_pubspec_configuration` read `pubspec.yaml`, including dependency sources,
  environment constraints, Flutter assets, fonts, and localization settings.
- The backend is heuristic: it does not run the Dart SDK and does not build a syntax tree. What it cannot
  decide it reports as a diagnostic or leaves out; `DartParserMetadata` states what each backend supports.
- The cost of one file is linear in its size. Pathological inputs end in a prefix result plus a
  `declaration_scan_truncated` or `invocation_facts_truncated` warning instead of unbounded work.

## Example

```rust
use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file;

let analysis = analyze_file(DartFileInput::new(
    "lib/a.dart",
    "import 'b.dart';\nclass A {}\n",
));
assert_eq!(analysis.imports.len(), 1);
assert_eq!(analysis.declarations[0].name, "A");
```

## Documentation

- [API reference](https://docs.rs/dartscope-parse)
- [Parser backends](https://github.com/RusTokRs/dartscope/blob/main/docs/development/parser-backends.md)
- [Fuzzing](https://github.com/RusTokRs/dartscope/blob/main/docs/development/fuzzing.md)
- [Analysis budgets](https://github.com/RusTokRs/dartscope/blob/main/docs/development/json-contracts.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
