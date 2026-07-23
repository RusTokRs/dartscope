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
symlinks are rejected before reading.

## Large repositories

The CLI budgets intentionally bound peak retained source text; they are not library API limits.
Applications that need a different ingestion policy should discover and stream files themselves,
then submit bounded batches or incremental updates through DartScope's in-memory APIs.
