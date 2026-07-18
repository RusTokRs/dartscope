#!/usr/bin/env python3
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "eb18e5b1b4306297797e635600676fbc26c833ae"
SOURCE_PATH = "tools/ds_quality001_dependency_gates_apply.py"
source = subprocess.check_output(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    text=True,
)
replacements = [
    (
        '''TESTS.write_text('''from __future__ import annotations''',
        '''TESTS.write_text(r'''from __future__ import annotations''',
    ),
    (
        '''import importlib.util
from pathlib import Path
import tempfile
''',
        '''import importlib.util
from pathlib import Path
import sys
import tempfile
''',
    ),
    (
        '''MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
''',
        '''MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)
''',
    ),
    (
        '''        "cargo audit": "RustSec audit command is missing",
        "cargo machete": "unused-dependency command is missing",
''',
        '''        "cargo +1.95.0 audit": "RustSec audit command is missing",
        "cargo +1.95.0 machete": "unused-dependency command is missing",
''',
    ),
    (
        '''            "cargo audit\\n"
            "cargo machete\\n",
''',
        '''            "cargo +1.95.0 audit\\n"
            "cargo +1.95.0 machete\\n",
''',
    ),
]
for old, new in replacements:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"reviewed dependency patch block count: {count}")
    source = source.replace(old, new)
exec(compile(source, SOURCE_PATH, "exec"), {"__name__": "__main__", "__file__": __file__})
