#!/usr/bin/env python3
"""Identify the exact dependency-fingerprint patch anchor that drifted."""

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "189dbf9f714341201c4f09dbcc3fb336724e84c9"
SOURCE_PATH = "tools/ds_index005_dependency_fingerprint_apply.py"

source = subprocess.run(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    check=True,
    stdout=subprocess.PIPE,
).stdout.decode("utf-8")

function_anchor = '''def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")
'''
function_replacement = '''def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        first = next((line for line in old.splitlines() if line.strip()), "<empty>")
        raise SystemExit(
            f"{path.relative_to(ROOT)}: expected one anchor, found {count}; first={first!r}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


def replace_first(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count < 1:
        first = next((line for line in old.splitlines() if line.strip()), "<empty>")
        raise SystemExit(
            f"{path.relative_to(ROOT)}: expected at least one anchor, found {count}; first={first!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
'''
if source.count(function_anchor) != 1:
    raise SystemExit("reviewed patcher function anchor drifted")
source = source.replace(function_anchor, function_replacement)

ambiguous_call = '''replace_once(
    INCREMENTAL,
    """            uri_graph,
            part_links,
            graphql_contracts,
""",
    """            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
""",
)
'''
if source.count(ambiguous_call) != 1:
    raise SystemExit("reviewed snapshot anchor call drifted")
source = source.replace(ambiguous_call, ambiguous_call.replace("replace_once(", "replace_first(", 1))

virtual_path = ROOT / SOURCE_PATH
exec(compile(source, str(virtual_path), "exec"), {"__name__": "__main__", "__file__": str(virtual_path)})
