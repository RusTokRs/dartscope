# dartscope-cli

The `dartscope` command-line tool: Dart and Flutter analysis as versioned JSON on stdout.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## Commands

| Command | Input | JSON contract |
| --- | --- | --- |
| `analyze-file` | Dart file | `dartscope.file-analysis` |
| `pubspec` | `pubspec.yaml` | `dartscope.pubspec-analysis` |
| `pubspec-config` | `pubspec.yaml` | `dartscope.pubspec-configuration` |
| `analyze-project` | project directory | `dartscope.project-analysis` |
| `graphql-contracts` | project directory | `dartscope.graphql-contracts` |
| `uri-graph` | project directory | `dartscope.uri-graph` |
| `flutter-inventory` | project directory | `dartscope.flutter-inventory` |
| `lint` | project directory | `dartscope.lint-analysis` or SARIF 2.1.0 |

## Install and run

```sh
cargo install dartscope-cli
dartscope analyze-project path/to/app --compact > analysis.json
```

Exit codes are stable (`0` success, `2` usage error, `3` unreadable input, `4`–`6` lint outcomes); errors go
to stderr. Input limits and skipped-input diagnostics are documented in the CLI contract.

## Documentation

- [API reference](https://docs.rs/dartscope-cli)
- [CLI contract](https://github.com/RusTokRs/dartscope/blob/main/docs/development/cli-contract.md)
- [Input limits](https://github.com/RusTokRs/dartscope/blob/main/docs/development/cli-input-limits.md)
- [Lint command](https://github.com/RusTokRs/dartscope/blob/main/docs/development/lint-cli.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
