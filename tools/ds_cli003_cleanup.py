#!/usr/bin/env python3
"""Record and guard the DS-CLI-003 finalization cleanup finding."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/development/dartscope-library-plan.md"

text = PLAN.read_text(encoding="utf-8")

implemented_old = """7. Documented direct GitHub Code Scanning upload without a custom converter.

Findings and limits:
"""
implemented_new = """7. Documented direct GitHub Code Scanning upload without a custom converter.
8. Removed Python bytecode accidentally captured by the verification runner, added repository ignore
   rules, and made repository consistency reject tracked `__pycache__` and compiled Python artifacts.

Findings and limits:
"""
if text.count(implemented_old) != 1:
    raise SystemExit("DS-CLI-003 implemented-list anchor was not found exactly once")
text = text.replace(implemented_old, implemented_new)

findings_old = """- The `toml` parser is a direct CLI dependency pinned by `Cargo.lock`; its public types do not cross the
  DartScope API boundary.

Acceptance:
"""
findings_new = """- The `toml` parser is a direct CLI dependency pinned by `Cargo.lock`; its public types do not cross the
  DartScope API boundary.
- **P1 fixed:** the first successful finalization staged Python bytecode created by policy-test imports.
  Generated Python artifacts are now ignored and a permanent repository-consistency check rejects any
  tracked recurrence.

Acceptance:
"""
if text.count(findings_old) != 1:
    raise SystemExit("DS-CLI-003 findings anchor was not found exactly once")
text = text.replace(findings_old, findings_new)

PLAN.write_text(text, encoding="utf-8")
print("DS-CLI-003 cleanup finding recorded")

# trigger revision 2
