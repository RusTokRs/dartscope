#!/usr/bin/env python3
"""Apply the per-library cache slice with borrow-safe GraphQL fixtures."""

import subprocess
import sys
import tempfile
from pathlib import Path

SOURCE_COMMIT = "23b9f8fe98570b1032757979eba8ba4ca1d34f19"
SOURCE_PATH = "tools/ds_index005_library_cache_apply_v2.py"
TESTS = Path("crates/dartscope-index/src/tests/incremental.rs")

result = subprocess.run(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    check=False,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
if result.returncode != 0:
    sys.stderr.buffer.write(result.stderr)
    raise SystemExit(result.returncode)

source = result.stdout.decode("utf-8")
with tempfile.TemporaryDirectory() as directory:
    patched = Path(directory) / "library_cache_apply_v2.py"
    patched.write_text(source, encoding="utf-8")
    namespace = {"__name__": "__main__", "__file__": str(patched)}
    exec(compile(source, str(patched), "exec"), namespace)

text = TESTS.read_text(encoding="utf-8")
old = "    let unresolved = &index.snapshot().graphql_contracts().unresolved_uses[0];\n"
if text.count(old) != 2:
    raise SystemExit(f"expected two temporary snapshot borrows, found {text.count(old)}")
text = text.replace(
    old,
    "    let initial_snapshot = index.snapshot();\n"
    "    let unresolved = &initial_snapshot.graphql_contracts().unresolved_uses[0];\n",
    1,
)
text = text.replace(
    old,
    "    let updated_snapshot = index.snapshot();\n"
    "    let unresolved = &updated_snapshot.graphql_contracts().unresolved_uses[0];\n",
    1,
)
TESTS.write_text(text, encoding="utf-8")
print("DS-INDEX-005 per-library cache slice v3 applied")
