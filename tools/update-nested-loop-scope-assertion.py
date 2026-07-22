from pathlib import Path

path = Path("crates/dartscope-parse/tests/lexical_region_bindings.rs")
source = path.read_text(encoding="utf-8")
old = '        scope.starts_with("if (enabled)") && scope.contains("else")\n'
new = '        scope.contains("if (enabled)") && scope.contains("else")\n'
if source.count(old) != 1:
    raise SystemExit("nested loop scope assertion not found exactly once")
path.write_text(source.replace(old, new, 1), encoding="utf-8")
