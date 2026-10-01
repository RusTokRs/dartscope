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

Only recognized inputs count toward the project budgets. Generated and tool directories from the
documented skip list are not traversed. `flutter-inventory` additionally counts `l10n.yaml` and
ARB catalogs; all other project commands count Dart, pubspec, and discovered package-config files.
Every item returned by `read_dir` counts toward the traversal limit before file type, skip-list,
or source-extension checks, so irrelevant files cannot bypass the CPU bound. Only non-skipped real
directories enter the pending queue. Limits are inclusive: an input or project exactly at its
configured byte or count boundary is accepted.

## Failure behavior

Limits are checked from the opened regular-file handle before allocation and checked again after
a bounded read. This prevents a file that grows during collection from bypassing the per-file or
aggregate budget. Limit failures are input errors (exit code 3) and use stable diagnostic prefixes:

- `input_file_too_large`
- `project_input_limit_exceeded`
- `project_traversal_limit_exceeded`

JSON is never partially written on a limit failure. The error is emitted only on stderr. Symlink
validation remains separate: in-root file symlinks are allowed, while escaping links and directory
symlinks are rejected before reading. Directories of the skip list (for example the Flutter
`.symlinks` and `.plugin_symlinks` folders) are not entered, so their links are not validated.
`analyze-project --skip-symlinks` reports a rejected symlink as the warning `input_symlink_skipped`
instead of failing, and always reports the directories of the skip list that hold generated or
third-party files as `input_directory_skipped` (severity `info`).
A `.dart` file that is not valid UTF-8 is skipped with the warning `input_file_not_utf8` by
`analyze-project` and rejected by every other command; it still counts toward the project budgets.

## Analysis budgets

The byte limits above bound what the CLI reads. Inside one file, the analysis has budgets of its own
that bound how much it copies and scans (they apply to the library as well). A file of nested calls
or of lines without a terminator is cut off with the warning `invocation_facts_truncated` or
`declaration_scan_truncated` instead of taking time and memory that grow with the square of its size;
see "Analysis budgets" in `json-contracts.md` for the numbers. Both are counted in bytes, so a run
over the same input gives the same result on every machine. The hostile-input sweep
(`crates/dartscope-parse/tests/adversarial_shapes.rs`, run by hand: 62 shapes of unclosed and nested
delimiters, long chains, huge tokens and runs of lines without terminators) shows linear growth for
every shape up to 1 MiB; a shape that is found to grow faster belongs in that list.

## Large repositories

The CLI budgets intentionally bound peak retained source text; they are not library API limits.
Applications that need a different ingestion policy should discover and stream files themselves,
then submit bounded batches or incremental updates through DartScope's in-memory APIs.
