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
dartscope lint <project> [--config <path>] [--format <json|sarif>] [--deny-warnings]
```

- No configuration path means `DartLintConfig::default()`: no rules are enabled and the command is
  inert.
- `--format json` is the default and emits `dartscope.lint-analysis` v1.
- `--format sarif` emits SARIF 2.1.0 with rule metadata, normalized artifact paths, exact available
  source regions, severities, and related-path evidence.
- `--deny-warnings` overrides the configured failure threshold for that invocation.
- Findings at the threshold still produce structured stdout and exit code `4`; stderr remains empty.

## TOML Configuration Version 1

The file is supplied explicitly with `--config`. Unknown fields, unsupported versions, duplicate rule
or severity entries, empty required values, and malformed TOML are configuration errors.

```toml
version = 1
failure_threshold = "error" # error, warning, or never
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
match_kind = "prefix" # prefix or exact
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
```

Configuration path prefixes accept `/` or `\`; the CLI normalizes them to `/` before invoking the
engine. Configuration order does not change rule execution or diagnostic ordering.

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
- SARIF artifact URIs are normalized project-relative paths; DartScope does not guess repository URI
  bases or checkout roots.
- Project error diagnostics stop lint execution at the first deterministic error message. Existing
  analysis commands retain their diagnostic-bearing success behavior.
- SARIF columns use `unicodeCodePoints`, matching DartScope's public source-span column semantics.
