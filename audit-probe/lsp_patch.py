#!/usr/bin/env python3
"""Probe-only: apply the minimal fixes needed to make dartscope-lsp compile so its wire behaviour can be observed."""
import pathlib
import re

root = pathlib.Path("crates/dartscope-lsp/src")
server = (root / "server.rs").read_text(encoding="utf-8")
count = server.count("from_snapshot(index.snapshot())")
server = server.replace("from_snapshot(index.snapshot())", "from_snapshot(&index.snapshot())")
(root / "server.rs").write_text(server, encoding="utf-8")

types = (root / "types.rs").read_text(encoding="utf-8")
patched = []
for name in ("Position", "Range", "Diagnostic"):
    pattern = re.compile(r"#\[derive\(([^)]*)\)\]\n(pub struct %s\b)" % name)
    match = pattern.search(types)
    if match and "Default" not in match.group(1):
        types = pattern.sub(lambda m: "#[derive(%s, Default)]\n%s" % (m.group(1), m.group(2)), types, count=1)
        patched.append(name)
(root / "types.rs").write_text(types, encoding="utf-8")
print(f"patched from_snapshot x{count}; derived Default for {patched}")
