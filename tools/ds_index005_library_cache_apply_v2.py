#!/usr/bin/env python3
"""Run the reviewed per-library cache patch with a contextual upsert anchor."""

import subprocess
import sys
import tempfile
from pathlib import Path

SOURCE_COMMIT = "b3729bb6ac0c18d5e632046bb32cef7d8445961d"
SOURCE_PATH = "tools/ds_index005_library_cache_apply.py"

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
old = '''replace_once(
    INCREMENTAL,
    """            changed_declaration_names,
        )
""",
    """            changed_declaration_names,
            changed_graphql_operation_names,
        )
""",
)
'''
new = '''replace_once(
    INCREMENTAL,
    """        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
        )
""",
    """        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
            changed_graphql_operation_names,
        )
""",
)
'''
if source.count(old) != 1:
    raise SystemExit(f"expected one ambiguous patcher block, found {source.count(old)}")
source = source.replace(old, new)

with tempfile.TemporaryDirectory() as directory:
    patched = Path(directory) / "library_cache_apply.py"
    patched.write_text(source, encoding="utf-8")
    namespace = {"__name__": "__main__", "__file__": str(patched)}
    exec(compile(source, str(patched), "exec"), namespace)
