---
id: doc://docs/development/lsp.md
kind: development_contract
language: en
source_language: en
status: active
---

# Language server: workspace model and limits

`dartscope-lsp` is an optional crate: a transport-independent JSON-RPC layer (`rpc`), the server state
(`DartLspServer`) and a stdio binary (`dartscope-lsp`). The server and the protocol layer do no I/O; the one
adapter that reads the filesystem is `FsWorkspace`, and the binary is the only caller that uses it. The
history of the crate and the audit that made it work against a real editor are in
`dartscope-library-plan.md` (DS-LSP-001).

## What the server knows

The index of the server holds two kinds of files, and an open document takes the place of its file:

- **Open documents.** The text the client sent with `textDocument/didOpen` and `didChange`. They are
  analyzed again after every change, and results name them with the URI the client wrote.
- **The project on disk.** When the client says it is ready (`initialized`), the server reads every Dart
  source, every `pubspec.yaml` and the `.dart_tool/package_config.json` next to it under the workspace
  folders (or under `rootUri`/`rootPath`), and analyzes them in one pass. Navigation then sees files
  that are not open, `package:` imports resolve through the package configuration or, without one, through
  the pubspec names, and `workspace/symbol` searches all of it. A buffer that is closed gives way to the
  file on disk again; a buffer that never was a file leaves the index.

The scan follows the rules of `dartscope analyze-project`: `.git`, `.dart_tool`, `.idea`, `.vscode`,
`.symlinks`, `.plugin_symlinks`, `Pods` and `node_modules` are not entered, `build`, `coverage` and `target` only
inside `lib`, `bin`, `test`, `test_driver`, `tool`, `integration_test` and `benchmark`, and symbolic links are
never followed. A file that is not UTF-8 is skipped.

## Requests and notifications

| Message | Behavior |
| --- | --- |
| `workspace/symbol` | Declarations of the project whose name contains the query in any letter case or has its letters in order (`hbs` finds `HomeBlocState`); exact matches first, then prefixes, substrings and the rest; at most 500 results; local variables are not listed. An empty query lists everything up to the limit. |
| `workspace/didChangeWatchedFiles` | A created or changed file is read again and replaces the loaded one; a deleted file (or one that cannot be read any more) is removed. The text of an open document is not replaced: the buffer is the truth until it is closed. A change of `pubspec.yaml` or `package_config.json` rebuilds the index. |
| `client/registerCapability` (sent) | After `initialized`, to a client that declares `workspace.didChangeWatchedFiles.dynamicRegistration`: watchers for `**/*.dart`, `**/pubspec.yaml` and `**/package_config.json`. A client that cannot watch files keeps the project as it was loaded, plus whatever it opens. |
| `window/logMessage` (sent) | A warning when the scan stopped at a limit or left large files out. |

## Limits

- **Size.** `MAX_NAVIGATION_BYTES` is 1 MiB. A larger open document keeps its outline and diagnostics but is
  not part of navigation (information diagnostic `navigation_disabled_large_file`); a larger file on disk is not
  loaded at all. The limit bounds the work done after every edit: the analysis of a file is linear and takes
  about a quarter of a second per MiB in a release build, and the analysis budgets
  (`json-contracts.md`) keep pathological text from costing more.
- **Scan.** At most 20,000 Dart files, 128 MiB of source and 250,000 directory entries; beyond that the scan
  stops and says so with a log message.
- **One thread.** Requests are handled one at a time. The scan runs on that thread between `initialized` and
  the first request, so a very large checkout delays the first answers; `$/cancelRequest` has no effect
  because there is never a queue of running work to cancel.
- **Project shape.** A `pubspec.yaml` in a subdirectory is a package of the project; a Dart workspace or a
  monorepo is read as one project with one index. Files that the project does not contain (the pub cache, the
  Dart SDK) are not read, so a definition in them is not found.
- **Unopened documents.** `documentSymbol`, `hover`, `definition` and `references` are asked about an open document; a
  request for a document the client never opened answers `null`.

## Verifying

`crates/dartscope-lsp/src/server/workspace.rs` (in-memory project), `src/rpc.rs` (the protocol with a project
in memory that changes between messages) and `tests/stdio.rs` (`the_project_on_disk_is_part_of_the_session`: the
real binary over pipes, a project in a temporary directory, a `package:` import into a file the client never
opens) cover the behavior above.
