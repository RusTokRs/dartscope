# dartscope-index

Deterministic project-level indexing over normalized DartScope analyses.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- A URI graph of imports, exports, and parts; part-of-library ownership; a namespace engine with
  conditional-compilation environments (`DartIndexOptions`).
- Symbol and identifier-reference resolution, `find_definitions` and `find_references` with inherited and
  extension members, and GraphQL contract linking across libraries.
- `DartWorkspaceIndex`, a stateful index: normalized upserts and removals, immutable `Arc` snapshots, reverse
  invalidation closures, and operation counters. Every snapshot equals a clean stateless rebuild.
- No filesystem access and no parser: the inputs are `dartscope-core` models.

## Example

```rust
use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::DartWorkspaceIndex;
use dartscope_parse::analyze_project;

let project = analyze_project(DartProjectInput::new(
    "/work/app",
    vec![DartFileInput::new("lib/a.dart", "class A {}\n")],
    Vec::new(),
));
let mut index = DartWorkspaceIndex::from_project(project);
let update = index.upsert_file(dartscope_parse::analyze_file(DartFileInput::new(
    "lib/a.dart",
    "class A {}\nclass B {}\n",
)));
println!("generation {} rebuilt {:?}", update.generation, update.rebuilt);
let snapshot = index.snapshot();
```

## Documentation

- [API reference](https://docs.rs/dartscope-index)
- [Incremental index](https://github.com/RusTokRs/dartscope/blob/main/docs/development/incremental-index.md)
- [Symbol resolution](https://github.com/RusTokRs/dartscope/blob/main/docs/development/symbol-resolution.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
