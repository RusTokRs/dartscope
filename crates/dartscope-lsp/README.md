# dartscope-lsp

A Language Server Protocol bridge over the incremental DartScope index: the `dartscope-lsp` stdio server and the library it is built from.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it does

- Definition, references, hover, document symbols, workspace symbols, and diagnostics, from the same
  deterministic analysis as the CLI. Results it cannot establish are empty rather than guessed.
- Loads the Dart files, `pubspec.yaml`, and `.dart_tool/package_config.json` of the workspace folders when the
  client is ready; open buffers override the disk, and `workspace/didChangeWatchedFiles` keeps the rest current.
- The protocol layer (`rpc`) and the server (`DartLspServer`) do no I/O of their own; `FsWorkspace` is the one
  adapter that reads the filesystem, with limits on the number and size of the files it loads.

## Run

```sh
cargo install dartscope-lsp
dartscope-lsp   # speaks LSP on stdin/stdout
```

## Documentation

- [API reference](https://docs.rs/dartscope-lsp)
- [Language server](https://github.com/RusTokRs/dartscope/blob/main/docs/development/lsp.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
