#!/usr/bin/env python3
"""Keep DS-CLI-003 verification status aligned with observed hosted evidence."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/development/dartscope-library-plan.md"
text = PLAN.read_text(encoding="utf-8")

header_old = """### DS-CLI-003: Lint Command, Configuration, And SARIF

Status: verified. Priority: P1. Prerequisite: DS-LINT-001.
"""
header_new = """### DS-CLI-003: Lint Command, Configuration, And SARIF

Status: implemented. Priority: P1. Prerequisite: DS-LINT-001.
"""
if text.count(header_old) != 1:
    raise SystemExit("DS-CLI-003 verified-status anchor was not found exactly once")
text = text.replace(header_old, header_new)

finding_old = """- **P1 fixed:** the first successful finalization staged Python bytecode created by policy-test imports.
  Generated Python artifacts are now ignored and a permanent repository-consistency check rejects any
  tracked recurrence.

Acceptance:
"""
finding_new = """- **P1 fixed:** the first successful finalization staged Python bytecode created by policy-test imports.
  Generated Python artifacts are now ignored and a permanent repository-consistency check rejects any
  tracked recurrence.
- **Verification pending:** the Rust 1.95 Ubuntu feature finalizer and focused cleanup gate passed, but
  GitHub did not publish the permanent Linux/Windows aggregate status for the final clean main SHA.
  Promote this task to `verified` only after a later clean main SHA reports `dartscope/ci` success.

Acceptance:
"""
if text.count(finding_old) != 1:
    raise SystemExit("DS-CLI-003 verification finding anchor was not found exactly once")
text = text.replace(finding_old, finding_new)

acceptance_old = """- exact Rust 1.95 formatting, focused tests, Clippy, rustdoc, workspace tests, umbrella all-features,
  release package validation, and hosted Linux/Windows checks pass.
"""
acceptance_new = """- exact Rust 1.95 formatting, focused tests, Clippy, rustdoc, workspace tests, umbrella all-features,
  and release package validation pass in the bounded finalizer;
- a clean permanent hosted Linux/Windows matrix reports aggregate `dartscope/ci` success before the
  task is promoted from `implemented` to `verified`.
"""
if text.count(acceptance_old) != 1:
    raise SystemExit("DS-CLI-003 acceptance anchor was not found exactly once")
text = text.replace(acceptance_old, acceptance_new)

PLAN.write_text(text, encoding="utf-8")
print("DS-CLI-003 hosted verification status corrected")

# trigger revision 2
