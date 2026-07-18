#!/usr/bin/env python3
"""Recover and apply the reviewed DS-QUALITY-001 v5 worktree patch for diagnostics."""

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "50140374835cdb6b0713153e376bdb662f6cab01"
SOURCE_PATH = "tools/ds_quality001_dependency_gates_apply_v5.py"

source = subprocess.check_output(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    text=True,
)
virtual_path = ROOT / SOURCE_PATH
exec(
    compile(source, str(virtual_path), "exec"),
    {"__name__": "__main__", "__file__": str(virtual_path)},
)
