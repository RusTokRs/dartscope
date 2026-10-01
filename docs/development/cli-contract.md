# CLI Contract

`dartscope` is a JSON-producing command-line interface over the public DartScope analysis APIs.
This document defines the stable process-level behavior for the 0.1 command family and the
additive `0.2` lint command.

## Global interface

- `dartscope --help`, `dartscope -h`, and `dartscope help` print global help to stdout.
- `dartscope --version` and `dartscope -V` print `dartscope <package-version>` to stdout.
- `dartscope help <command>` and `dartscope <command> --help` print command-specific help.
- Successful analysis commands write one versioned JSON envelope to stdout and write nothing to
  stderr.
- `--compact` (every analysis command, after the path, at most once) writes the same JSON document
  on one line instead of indented. Only whitespace between tokens differs, so the two forms parse to
  the same value; the default stays indented.
- Argument and input errors write nothing to stdout and one human-readable error to stderr.

The CLI is built with the optional Flutter feature. `analyze-file` and `analyze-project`
explicitly compose pure parser results with `dartscope-flutter` conventions before serialization;
`flutter-inventory` derives the same conventions directly from normalized project facts. This is a
CLI composition choice, not behavior owned by `dartscope-parse`.

The supported commands are:

| Command | Input | Optional arguments | JSON schema |
| --- | --- | --- | --- |
| `analyze-file` | Dart file | `--compact` | `dartscope.file-analysis` |
| `pubspec` | `pubspec.yaml` | `--compact` | `dartscope.pubspec-analysis` |
| `pubspec-config` | `pubspec.yaml` | `--compact` | `dartscope.pubspec-configuration` |
| `analyze-project` | project directory | `--relative-root`, `--skip-symlinks`, `--compact` | `dartscope.project-analysis` |
| `graphql-contracts` | project directory | repeatable `--env key=value`, `--compact` | `dartscope.graphql-contracts` |
| `uri-graph` | project directory | repeatable `--env key=value`, `--compact` | `dartscope.uri-graph` |
| `flutter-inventory` | project directory | `--compact` | `dartscope.flutter-inventory` |
| `lint` | project directory | `--config`, `--format`, `--deny-warnings`, `--compact` | `dartscope.lint-analysis` or SARIF 2.1.0 |

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The requested help, version, or JSON operation completed successfully. |
| `1` | DartScope could not serialize or otherwise complete an internal operation, or could not write its output for a reason other than a closed pipe. |
| `2` | The command line is invalid: unknown command, missing path, unexpected option, malformed `--env`, or an argument that is not valid Unicode. |
| `3` | A requested file, project directory, or lint configuration cannot be read. |
| `4` | Lint structured output was emitted and a finding reached the configured failure threshold. |
| `5` | Lint TOML configuration is malformed, unsupported, or semantically invalid. |
| `6` | Lint project analysis produced an error diagnostic and rule execution was not trusted. |

When the reader of standard output goes away (`dartscope analyze-project . | head`), the command stops
writing and exits with the exit code of its own result instead of failing.

Malformed Dart, YAML, and package-configuration contents remain diagnostic-bearing success inputs
for the original analysis commands. The `lint` command uses exit code `6` instead because running
policy rules over an error-bearing project would claim more confidence than the normalized input
supports. Lint findings at exit code `4` remain structured stdout, not stderr errors.

## Project discovery

Project commands recursively visit regular files under the explicitly supplied root. Paths are
normalized to forward slashes in analysis inputs and sorted before analysis, so traversal order is
stable across Linux and Windows.

`data.root` of `analyze-project` is the absolute path of the project directory with its `.`
components dropped: `dartscope analyze-project .` run in `/work/app` reports `/work/app`, not
`/work/app/.`. `--relative-root` reports `.` instead, so the document does not depend on where the
project was checked out and can be compared or cached across machines; file paths are relative to the
root either way.

Each discovered `pubspec.yaml` owns the nearest sibling `.dart_tool/package_config.json` below the
same package directory. This supports nested packages without borrowing a package configuration
from a parent package.

A symbolic link to a file whose target stays inside the project root is read like the file it points
to; this includes a symlinked package-config file. A link whose target leaves the root, a link to a
directory, and a link that cannot be resolved fail the run with exit code `3` and a message that
starts with `input_symlink_rejected`: the CLI does not follow anything it cannot show to be inside
the root. `analyze-project --skip-symlinks` leaves such a link out instead and reports the warning
`input_symlink_skipped` with its path and the reason, so one stray link does not hide the rest of a
monorepo; the other commands cannot carry that report and keep failing. Directories in the skip lists below are never entered, so the links that Flutter and
CocoaPods create inside them do not matter. An explicitly supplied project root may be a symlink
because it is an intentional user-selected boundary.

The recursive walker never enters these generated, dependency or tool-owned directories, whatever
their location (`.symlinks` and `.plugin_symlinks` hold the links that `pod install` and Flutter
create into the pub cache and into plugin checkouts):

```text
.dart_tool
.git
.idea
.pub-cache
.vscode
.symlinks
.plugin_symlinks
node_modules
Pods
```

`build`, `coverage` and `target` hold generated output next to a package, but are ordinary folder
names inside the source roots, so they are skipped only when no directory between the project root
and the folder is one of `lib`, `bin`, `test`, `test_driver`, `tool`, `integration_test` or
`benchmark`. `lib/src/build/steps.dart` is analyzed; `build/generated.dart` and
`packages/app/build/x.dart` are not.

`analyze-project` names the skipped directories that a reader might expect to be analyzed: one
`input_directory_skipped` diagnostic of severity `info` with the directory path for each `build`,
`coverage`, `target`, `Pods`, `node_modules`, `.symlinks` and `.plugin_symlinks` the walk did not
enter (counted in `summary.diagnostics`). Tool state (`.git`, `.dart_tool`, `.idea`, `.pub-cache`,
`.vscode`) is skipped silently, and so is a source folder that happens to be named `build`.

A `.dart` file that is not valid UTF-8 is not Dart source the analysis can describe. `analyze-project`
leaves it out and reports the warning `input_file_not_utf8` with the file path (counted in
`summary.diagnostics`) so that one stray file does not hide the rest of the project; the other
commands, including `lint`, reject it with exit code `3` (`failed to read`).

Paths containing spaces are supported as normal OS arguments. The CLI does not perform shell
splitting of path or environment values.
