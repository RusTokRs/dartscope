# dartscope-core

Normalized analysis models shared by the DartScope crates: spans, declarations, directives, invocations, GraphQL operations, identifier references, diagnostics, and the `pubspec.yaml` model.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- Data and constructors only. The crate does no parsing and no filesystem access.
- Every type serializes with `serde`; the JSON shapes behind the CLI are documented as contracts.
- `normalize_path` turns backslashes into forward slashes; every path in a model is normalized this way.

## Example

```rust
use dartscope_core::{DartFileInput, DartProjectInput};

let file = DartFileInput::new("lib/main.dart", "void main() {}");
let project = DartProjectInput::new("/work/app", vec![file], Vec::new());
assert_eq!(project.files.len(), 1);
```

## Documentation

- [API reference](https://docs.rs/dartscope-core)
- [JSON contracts](https://github.com/RusTokRs/dartscope/blob/main/docs/development/json-contracts.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
