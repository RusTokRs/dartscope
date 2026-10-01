---
id: doc://docs/development/lint-cli.md
kind: development_contract
language: en
source_language: en
status: active
---

# Lint CLI, Configuration, And SARIF

`dartscope lint <project>` is the filesystem/process adapter for the source-free
`dartscope-lints` engine. The command discovers the project with the same deterministic,
non-symlink-following walker as other project commands, analyzes normalized facts, and then calls
`lint_project`. Rule semantics remain in `dartscope-lints`.

## Command

```text
dartscope lint <project> [--config <path>] [--format <json|sarif>] [--deny-warnings] [--compact]
```

- No configuration path means `DartLintConfig::default()`: no rules are enabled and the command is
  inert, so a project is only linted against what its configuration names.
- A configuration file that enables no rule is a configuration error (exit code `5`): the file
  exists to name what is checked, and a run that checks nothing looks the same as a clean project.
- `dartscope.orphan_file` needs at least one `[orphan_files].entry_points` entry. Enabling it with an
  empty list is a configuration error (exit code `5`), and a listed entry point that is not an
  analyzed Dart file is reported as a finding instead of silently disabling the rule.
- `--format json` is the default and emits `dartscope.lint-analysis` v1.
- `--format sarif` emits SARIF 2.1.0 with rule metadata, normalized artifact paths, exact available
  source regions, severities, and related-path evidence.
- `--deny-warnings` overrides the configured failure threshold for that invocation.
- `--compact` prints the JSON or SARIF document on one line (the same document without the
  indentation).
- Findings at the threshold still produce structured stdout and exit code `4`; stderr remains empty.

## TOML Configuration Version 1

The file is supplied explicitly with `--config`. Unknown fields, unsupported versions, duplicate rule
or severity entries, empty required values, and malformed TOML are configuration errors.

```toml
version = 1
failure_threshold = "error" # error, warning, or never
path_match = "string" # string (default) or segment; see "Path matching"
enabled_rules = [
  "dartscope.forbidden_import",
  "dartscope.layer_boundary",
  "dartscope.naming_convention",
  "dartscope.unresolved_part",
  "dartscope.orphan_file",
]

[[severity_overrides]]
rule_id = "dartscope.forbidden_import"
severity = "error"

[[forbidden_imports]]
uri = "package:legacy/"
match_kind = "prefix" # prefix (default), segment_prefix or exact
source_prefix = "lib/"

[[layer_boundaries]]
source_prefix = "lib/ui/"
denied_target_prefixes = ["lib/data/", "lib/infrastructure/"]

[naming]
check_file_names = true
check_top_level_declarations = true
ignored_path_prefixes = ["lib/generated/"]

[orphan_files]
entry_points = ["lib/main.dart"]
ignored_path_prefixes = ["test/fixtures/"]

[exclude] # no rule reports on these files
path_prefixes = ["lib/generated/"]
path_suffixes = [".g.dart", ".freezed.dart"]
```

Configuration paths accept `/` or `\`; the CLI normalizes them to `/` before invoking the engine.
Configuration order does not change rule execution or diagnostic ordering.

### Path matching

`path_match` decides how every configured path prefix is compared with a normalized project path:
`source_prefix`, `denied_target_prefixes`, `[naming].ignored_path_prefixes`,
`[orphan_files].ignored_path_prefixes` and `[exclude].path_prefixes`.

- `"string"` (the default, and the only behavior of earlier configurations) is a plain string
  prefix: `lib/ui` also covers `lib/ui_kit/button.dart`, so write `lib/ui/` to name the directory.
- `"segment"` compares whole path segments: `lib/ui` and `lib/ui/` both cover `lib/ui` and
  `lib/ui/button.dart`, and neither covers `lib/ui_kit/button.dart`.

A forbidden import pattern with `match_kind = "segment_prefix"` does the same for URIs:
`package:flutter` matches `package:flutter` and `package:flutter/material.dart`, but not
`package:flutter_bloc/flutter_bloc.dart`, which the default `prefix` kind also matches.

### Exclusions

`[exclude]` removes findings about generated or vendored files without touching the rules: the rules
run as usual, and a finding whose path is under one of `path_prefixes` (compared as `path_match`
says) or ends with one of `path_suffixes` (a plain string suffix such as `.g.dart`) is dropped
before the output is built, so `summary` counts only what is reported. The exclusions apply to
every rule, including `dartscope.orphan_file` and the unresolved-part findings.

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Analysis completed and no finding reached the configured threshold. |
| `1` | Internal serialization or execution failure. |
| `2` | Invalid command line. |
| `3` | Project or configuration file could not be read. |
| `4` | Structured lint output was emitted and at least one finding reached the threshold. |
| `5` | TOML configuration is malformed or semantically invalid. |
| `6` | Project analysis produced an error diagnostic, so lint execution was not trusted. |

## GitHub Code Scanning

The SARIF stream needs no custom transformation. Pin Actions to reviewed immutable commits in the
consuming repository:

```yaml
- name: Run DartScope lints
  id: dartscope_lint
  shell: bash
  run: |
    set +e
    cargo run --locked -p dartscope-cli -- \
      lint . --config dartscope.toml --format sarif --deny-warnings \
      > dartscope.sarif
    status=$?
    echo "exit_code=$status" >> "$GITHUB_OUTPUT"
    if [ "$status" -ne 0 ] && [ "$status" -ne 4 ]; then
      exit "$status"
    fi

- name: Upload DartScope SARIF
  uses: github/codeql-action/upload-sarif@<reviewed-immutable-commit-sha>
  with:
    sarif_file: dartscope.sarif

- name: Fail on DartScope findings
  if: steps.dartscope_lint.outputs.exit_code == '4'
  run: exit 4
```

The lint step converts only exit code `4` into a temporary successful step so the SARIF upload can run,
then the final step restores the finding failure. Filesystem, configuration, malformed-project, usage,
and internal failures stop the job before upload instead of publishing incomplete results.

## Current Limits

- TOML configuration version updates are manual and require a documented migration.
- SARIF artifact URIs are normalized project-relative paths, percent-encoded as URIs (a path with a
  space or a non-ASCII letter stays a valid URI reference); DartScope does not guess repository URI
  bases or checkout roots. A finding without a source span gets a region on line 1, because code
  scanning needs a location for every result.
- Project error diagnostics stop lint execution at the first deterministic error message. Existing
  analysis commands retain their diagnostic-bearing success behavior.
- SARIF columns use `unicodeCodePoints`, matching DartScope's public source-span column semantics.
