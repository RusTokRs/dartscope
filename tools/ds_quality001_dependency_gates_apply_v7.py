#!/usr/bin/env python3
"""Recover and run the reviewed DS-QUALITY-001 dependency gate v6 patch."""

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "8b6c83957ba07bb428a40c3ae94787657acb2247"
SOURCE_PATH = "tools/ds_quality001_dependency_gates_apply_v6.py"

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
