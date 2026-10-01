# dartscope-json

The stable JSON boundary of DartScope: a registry of named contracts and a versioned envelope.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- `JsonContract` lists the eight command-facing contracts (`dartscope.file-analysis`,
  `dartscope.pubspec-analysis`, `dartscope.pubspec-configuration`, `dartscope.project-analysis`,
  `dartscope.graphql-contracts`, `dartscope.uri-graph`, `dartscope.flutter-inventory`,
  `dartscope.lint-analysis`), each with a schema identifier and a major version.
- `to_json_contract` and `to_json_contract_pretty` wrap a value in a `VersionedJsonEnvelope`. `to_json` and
  `to_json_pretty` are plain serde output and are not a versioned schema.

## Example

```rust,ignore
use dartscope_json::{JsonContract, to_json_contract};

let json = to_json_contract(JsonContract::FileAnalysis, &analysis)?;
```

## Documentation

- [API reference](https://docs.rs/dartscope-json)
- [JSON contracts](https://github.com/RusTokRs/dartscope/blob/main/docs/development/json-contracts.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
