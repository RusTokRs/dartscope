# CLI Contract

`dartscope` is a JSON-producing command-line interface over the public DartScope analysis APIs.
This document defines the stable process-level behavior for the 0.1 command family.

## Global interface

- `dartscope --help`, `dartscope -h`, and `dartscope help` print global help to stdout.
- `dartscope --version` and `dartscope -V` print `dartscope <package-version>` to stdout.
- `dartscope help <command>` and `dartscope <command> --help` print command-specific help.
- Successful analysis commands write one versioned JSON envelope to stdout and write nothing to
  stderr.
- Argument and input errors write nothing to stdout and one human-readable error to stderr.

The supported commands are:

| Command | Input | Optional arguments | JSON schema |
| --- | --- | --- | --- |
| `analyze-file` | Dart file | none | `dartscope.file-analysis` |
| `pubspec` | `pubspec.yaml` | none | `dartscope.pubspec-analysis` |
| `pubspec-config` | `pubspec.yaml` | none | `dartscope.pubspec-configuration` |
| `analyze-project` | project directory | none | `dartscope.project-analysis` |
| `graphql-contracts` | project directory | repeatable `--env key=value` | `dartscope.graphql-contracts` |
| `uri-graph` | project directory | repeatable `--env key=value` | `dartscope.uri-graph` |
| `flutter-inventory` | project directory | none | `dartscope.flutter-inventory` |

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The requested help, version, or JSON operation completed successfully. |
| `1` | DartScope could not serialize or otherwise complete an internal operation. |
| `2` | The command line is invalid: unknown command, missing path, unexpected option, or malformed `--env`. |
| `3` | A requested file or project directory cannot be read or has the wrong filesystem type. |

Malformed Dart, YAML, and package-configuration contents are analysis inputs rather than CLI
argument errors. They produce a successful JSON envelope containing diagnostics instead of a
process panic.

## Project discovery

Project commands recursively visit regular files under the explicitly supplied root. Paths are
normalized to forward slashes in analysis inputs and sorted before analysis, so traversal order is
stable across Linux and Windows.

Each discovered `pubspec.yaml` owns the nearest sibling `.dart_tool/package_config.json` below the
same package directory. This supports nested packages without borrowing a package configuration
from a parent package.

Directory entries that are symbolic links are not followed. An explicitly supplied project root
may be a symlink because it is an intentional user-selected boundary, but symlinks encountered
inside that root are ignored. A symlinked package-config file is also ignored.

The recursive walker skips these generated or tool-owned directories by exact name:

```text
.dart_tool
.git
.idea
.pub-cache
.vscode
build
coverage
node_modules
Pods
target
```

Paths containing spaces are supported as normal OS arguments. The CLI does not perform shell
splitting of path or environment values.
