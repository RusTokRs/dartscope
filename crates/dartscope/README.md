# dartscope

Thin umbrella crate: one dependency and one import path for the DartScope libraries.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## Features

| Feature | Re-exports | Default |
| --- | --- | --- |
| *(always)* | `dartscope-core` models | yes |
| `parse` | `dartscope-parse` | yes |
| `resolve` | `dartscope-resolve` | yes |
| `index` | `dartscope-index` | yes |
| `json` | `dartscope-json` | yes |
| `lints` | `dartscope-lints` | no |
| `flutter` | `dartscope-flutter` | no |
| `lsp` | `dartscope-lsp` | no |

## Example

```rust
use dartscope::{DartFileInput, analyze_file};

let analysis = analyze_file(DartFileInput::new("lib/main.dart", "void main() {}"));
assert!(analysis.diagnostics.is_empty());
```

## Documentation

- [API reference](https://docs.rs/dartscope)
- [Library plan](https://github.com/RusTokRs/dartscope/blob/main/docs/development/dartscope-library-plan.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
