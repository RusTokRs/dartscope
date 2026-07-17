#!/usr/bin/env python3
"""Apply the reviewed DS-INDEX-005 semantic correction and its Clippy fix."""

from pathlib import Path
import subprocess
import sys
import tempfile

SOURCE_COMMIT = "7d258bbefc0d8268c8170100469bb7e50691693f"
SOURCE_PATH = "tools/ds_index005_semantic_fix.py"
INCREMENTAL = Path("crates/dartscope-index/src/incremental.rs")

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

lines = INCREMENTAL.read_text(encoding="utf-8").splitlines()
needle = "        .filter_map(|(path, references)| {"
matches = [index for index, line in enumerate(lines) if line == needle]
if len(matches) != 1:
    raise SystemExit(f"expected one filter_map_bool_then block, found {len(matches)}")
index = matches[0]
expected = [
    "        .filter_map(|(path, references)| {",
    "            references",
    "                .iter()",
    "                .any(|reference| names.contains(&reference.name))",
    "                .then(|| path.clone())",
    "        })",
]
if lines[index : index + len(expected)] != expected:
    raise SystemExit("filter_map_bool_then block shape changed")
replacement = [
    "        .filter(|(_, references)| {",
    "            references",
    "                .iter()",
    "                .any(|reference| names.contains(&reference.name))",
    "        })",
    "        .map(|(path, _)| path.clone())",
]
lines[index : index + len(expected)] = replacement
INCREMENTAL.write_text("\n".join(lines) + "\n", encoding="utf-8")
print("DS-INDEX-005 semantic correction v2 applied")
