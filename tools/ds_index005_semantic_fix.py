#!/usr/bin/env python3
"""Recover and run the reviewed DS-INDEX-005 semantic patch from Git history."""

from pathlib import Path
import subprocess
import sys
import tempfile

SOURCE_COMMIT = "7d258bbefc0d8268c8170100469bb7e50691693f"
SOURCE_PATH = "tools/ds_index005_semantic_fix.py"

with tempfile.TemporaryDirectory() as directory:
    recovered = Path(directory) / "semantic_fix.py"
    result = subprocess.run(
        ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        sys.stderr.buffer.write(result.stderr)
        raise SystemExit(result.returncode)
    recovered.write_bytes(result.stdout)
    namespace = {"__name__": "__main__", "__file__": str(recovered)}
    exec(compile(result.stdout, str(recovered), "exec"), namespace)
