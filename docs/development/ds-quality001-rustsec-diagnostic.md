---
id: doc://docs/development/ds-quality001-rustsec-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-QUALITY-001 RustSec Diagnostic

- apply exit: `0`
- Rust install exit: `0`
- cargo-audit install exit: `0`
- audit exit: `1`

```text
error: cargo-audit fatal error: parse error: TOML parse error at line 6, column 1
  |
6 | [output]
  | ^^^^^^^^
missing field `quiet`

```
