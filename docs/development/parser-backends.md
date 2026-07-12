# Parser Backends

`dartscope-parse` accepts source text through `DartFileInput` and emits normalized
`DartFileAnalysis`. A backend implements `DartParser`; it does not expose an AST and it does
not perform filesystem I/O. Consumers that need an alternative parser call
`analyze_project_with_parser` with their implementation.

```rust
use dartscope_parse::{DartParser, DartParserMetadata};

struct ExternalParser;

impl DartParser for ExternalParser {
    fn metadata(&self) -> DartParserMetadata {
        // Declare every supported and unsupported capability here.
        todo!()
    }

    fn analyze_file(&self, input: dartscope_core::DartFileInput)
        -> dartscope_core::DartFileAnalysis {
        // Convert external parser output into DartScope's stable core model.
        todo!()
    }
}
```

The built-in `HeuristicDartParser` remains the default behind `analyze_file` and
`analyze_project`. It reports `Members` as unsupported and makes only partial Dart
language-version coverage claims.

A future tree-sitter backend should retain its tree internally and map only supported facts to
`dartscope-core`. An official analyzer bridge should run outside `dartscope-core`, accept the
same in-memory inputs, convert analyzer diagnostics/spans to core types, and declare every
missing capability in `DartParserMetadata` rather than returning an indistinguishable empty list.
