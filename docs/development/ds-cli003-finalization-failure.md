---
id: doc://docs/development/ds-cli003-finalization-failure.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-CLI-003 Finalization Failure

Failed stage: `apply implementation (exit 1)`.

The implementation was not committed. Temporary staging files were removed.

```text
===== decode implementation =====
===== apply implementation =====
crates/dartscope-cli/src/main.rs: expected one replacement, found 0
```
