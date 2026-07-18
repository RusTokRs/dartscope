---
id: doc://docs/development/ds-index005-cache-baseline-apply-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Cache Baseline Apply Diagnostic

Apply exit: `1`.

```text
Traceback (most recent call last):
  File "/tmp/apply.py", line 25, in <module>
    replace_once(
  File "/tmp/apply.py", line 18, in replace_once
    text = path.read_text(encoding="utf-8")
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.12/pathlib.py", line 1029, in read_text
    with self.open(mode='r', encoding=encoding, errors=errors) as f:
         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.12/pathlib.py", line 1015, in open
    return io.open(self, mode, buffering, encoding, errors, newline)
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
FileNotFoundError: [Errno 2] No such file or directory: '/crates/dartscope-index/src/incremental.rs'
```
