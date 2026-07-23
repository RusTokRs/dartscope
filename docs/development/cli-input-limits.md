# CLI input limits

DartScope's library APIs remain filesystem-free and accept caller-owned in-memory inputs. The
`dartscope` executable is the filesystem boundary, so it applies fixed defensive budgets before
source text is retained for analysis.

## Default budgets

- Each Dart, pubspec, package-configuration, `l10n.yaml`, or ARB input: **8 MiB**.
- Each direct `analyze-file`, `pubspec`, or `pubspec-config` input: **8 MiB**.
- Lint configuration TOML: **1 MiB**.
- Project collection: **20,000 loaded inputs** and **256 MiB aggregate source bytes**.
- Project traversal: **250,000 inspected directory entries** and **25,000 pending directories**.
- Retained structured-analysis results: **2,000,000 counted collection items per command**.
- Structured JSON or SARIF output: **128 MiB**.

Only recognized inputs count toward the project budgets. Generated and tool directories from the
documented skip list are not traversed. `flutter-inventory` additionally counts `l10n.yaml` and
ARB catalogs; all other project commands count Dart, pubspec, and discovered package-config files.
Every item returned by `read_dir` counts toward the traversal limit before file type, skip-list,
or source-extension checks, so irrelevant files cannot bypass the CPU bound. Only non-skipped real
directories enter the pending queue. One retained-result budget is shared across every in-memory
analysis stage used by a command. It counts major collection entries such as file and project facts,
pubspec and package-configuration entries, URI references and candidate paths, GraphQL contracts,
Flutter inventory entries, and lint diagnostics. Nested collection entries are charged as well.
Structured output is serialized into a bounded in-memory buffer before stdout is touched. Limits are
inclusive: an input, project, result, or output document exactly at its configured boundary is accepted.

## Failure behavior

Limits are checked from the opened regular-file handle before allocation and checked again after
a bounded read. This prevents a file that grows during collection from bypassing the per-file or
aggregate budget. On Unix and Windows, the final path component is opened with platform no-follow
semantics. Direct file commands first resolve an already-existing symlink target, while project
collection opens the exact target that passed project-root validation. Replacing that final component
with a symlink before `open` fails deterministically instead of following the replacement.

Limit failures and path-race failures are input errors (exit code 3) and use stable diagnostic prefixes:

- `input_file_too_large`
- `input_path_changed`
- `project_input_limit_exceeded`
- `project_traversal_limit_exceeded`
- `analysis_result_limit_exceeded`
- `analysis_output_limit_exceeded`

JSON and SARIF are never partially written on a limit failure. A result-cardinality failure occurs
before serialization, and an oversized serialized buffer is discarded before stdout is touched. The
retained-result budget is normally checked after each producer finishes its current in-memory stage.
The `uri-graph` command additionally reserves the exact top-level reference count before graph
construction, so an oversized reference vector is rejected before that producer allocates it.
Candidate-path vectors are still checked immediately after graph construction. Other producers remain
post-stage guarded and do not yet have producer-side allocation budgets. Symlink validation remains
separate: in-root file symlinks are allowed, while escaping links and directory symlinks are rejected
before reading. The no-follow open closes replacement of the final file component. It does not claim
protection from concurrent replacement of an ancestor directory component; eliminating that broader
race requires a capability-directory or `openat`-style traversal redesign.

## Large repositories

The CLI budgets intentionally bound peak retained source text; they are not library API limits.
Applications that need a different ingestion policy should discover and stream files themselves,
then submit bounded batches or incremental updates through DartScope's in-memory APIs.
