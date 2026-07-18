---
id: doc://docs/development/ds-quality001-v5-compile-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-QUALITY-001 V5 Compile Diagnostic

- apply exit: `0`
- Rust install exit: `0`
- parse check exit: `101`
- workspace Clippy exit: `101`

## Apply
```text
tools/ds_quality001_dependency_gates_apply.py:59: SyntaxWarning: invalid escape sequence '\.'
DS-QUALITY-001 dependency gate slice applied
DS-QUALITY-001 dependency gate v5 slice applied
```

## Parse check
```text
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: component clippy is up to date
info: downloading component rustfmt
    Updating crates.io index
error: cannot update the lock file /home/runner/work/dartscope/dartscope/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
```

## Workspace Clippy
```text
error: cannot update the lock file /home/runner/work/dartscope/dartscope/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
```
