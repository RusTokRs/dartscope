#!/usr/bin/env python3
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "189dbf9f714341201c4f09dbcc3fb336724e84c9"
SOURCE_PATH = "tools/ds_index005_dependency_fingerprint_apply.py"

source = subprocess.check_output(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    text=True,
)
old = '''replace_once(
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
new = '''replace_once(
    INCREMENTAL,
    """        let snapshot = Arc::new(DartWorkspaceSnapshot {
            generation: 0,
            project,
            uri_graph,
            part_links,
            graphql_contracts,
            identifier_reference_resolutions,
        });
""",
    """        let snapshot = Arc::new(DartWorkspaceSnapshot {
            generation: 0,
            project,
            uri_graph,
            part_links,
            library_dependency_fingerprints,
            graphql_contracts,
            identifier_reference_resolutions,
        });
""",
)
'''
count = source.count(old)
if count != 1:
    raise SystemExit(f"reviewed patcher block count: {count}")
source = source.replace(old, new)
exec(compile(source, SOURCE_PATH, "exec"), {"__name__": "__main__", "__file__": __file__})
