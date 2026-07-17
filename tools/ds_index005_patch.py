#!/usr/bin/env python3
"""Patch the staged DS-INDEX-005 payload after the Rust 1.95 compile diagnostic."""

from pathlib import Path

APPLIER = Path("tools/ds_index005_apply.py")
source = APPLIER.read_text(encoding="utf-8")
old = "!consumed[*index] && *candidate == diagnostic"
new = "!consumed[*index] && **candidate == diagnostic"
if source.count(old) != 1:
    raise SystemExit(f"expected one diagnostic comparison anchor, found {source.count(old)}")
APPLIER.write_text(source.replace(old, new), encoding="utf-8")
print("DS-INDEX-005 diagnostic comparison patched")
