#!/usr/bin/env python3
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "8dd4ba68eccdc284af567a55d0a2d1a0b4c82519"
SOURCE_PATH = "tools/ds_index005_cache_baseline_apply.py"
source = subprocess.check_output(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    text=True,
)
old = '''replace_once(
    UMBRELLA,
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceSnapshot,
""",
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceIndexRetainedMetrics, DartWorkspaceSnapshot,
""",
)
'''
new = '''replace_once(
    UMBRELLA,
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceSnapshot, DartWorkspaceSubsystems,
""",
    """    DartIndexOptions, DartLibraryDependencyFingerprint, DartWorkspaceIndex,
    DartWorkspaceIndexCounters, DartWorkspaceIndexRetainedMetrics, DartWorkspaceSnapshot,
    DartWorkspaceSubsystems,
""",
)
'''
count = source.count(old)
if count != 1:
    raise SystemExit(f"reviewed umbrella patch block count: {count}")
source = source.replace(old, new)
exec(compile(source, SOURCE_PATH, "exec"), {"__name__": "__main__", "__file__": __file__})
