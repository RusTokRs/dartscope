#!/usr/bin/env python3
"""Compare DartScope declarations/directives with the official Dart parser over a real-world corpus."""
import collections
import json
import os
import subprocess
import sys
import time

BIN = sys.argv[1]
FILELIST = sys.argv[2]
REF = sys.argv[3]
OUT = sys.argv[4]
MAX_EXAMPLES = 6

ref_rows = json.load(open(REF))
files = [l.strip() for l in open(FILELIST) if l.strip()]
ref_by_file = collections.defaultdict(list)
syntax_errors = {}
ref_errors = {}
for row in ref_rows:
    if "err" in row:
        ref_errors[row["f"]] = row["err"]
    elif "syntax_errors" in row:
        syntax_errors[row["f"]] = row["syntax_errors"]
    else:
        ref_by_file[row["f"]].append(row)

KIND_MAP = {"extension_type": "extension_type"}
ds_by_file = {}
ds_failures = []
slow = []
t_total = 0.0
for path in files:
    started = time.time()
    proc = subprocess.run([BIN, "analyze-file", path], capture_output=True, timeout=120)
    elapsed = time.time() - started
    t_total += elapsed
    if elapsed > 2:
        slow.append((round(elapsed, 2), path))
    if proc.returncode != 0:
        ds_failures.append((path, proc.returncode, proc.stderr.decode("utf-8", "replace")[:120]))
        continue
    data = json.loads(proc.stdout)["data"]
    names = {d.get("symbol_id"): d["name"] for d in data["declarations"]}
    rows = []
    for d in data["declarations"]:
        if d["kind"] == "local_variable":
            continue
        owner = names.get(d.get("parent_symbol_id"), "") if d.get("parent_symbol_id") else ""
        rows.append({"k": d["kind"], "n": d["name"], "o": owner, "l": d["span"]["start_line"]})
    for kind, key in (("import", "imports"), ("export", "exports"), ("part", "parts")):
        for item in data[key]:
            rows.append({"k": kind, "n": item["uri"], "o": "", "l": item["span"]["start_line"]})
    ds_by_file[path] = rows

def key(row):
    return (row["k"], row["n"], row["o"])

stats = collections.defaultdict(lambda: {"ref": 0, "ds": 0, "match": 0})
missing = collections.defaultdict(list)
extra = collections.defaultdict(list)
compared_files = 0
for path in files:
    if path not in ds_by_file or path in ref_errors:
        continue
    if syntax_errors.get(path, 0) > 0:
        continue  # the official parser itself reports errors: not a fair comparison
    compared_files += 1
    ref = collections.Counter(key(r) for r in ref_by_file[path])
    ds = collections.Counter(key(r) for r in ds_by_file[path])
    ref_lines = {key(r): r["l"] for r in ref_by_file[path]}
    ds_lines = {key(r): r["l"] for r in ds_by_file[path]}
    for k, n in ref.items():
        stats[k[0]]["ref"] += n
        m = min(n, ds.get(k, 0))
        stats[k[0]]["match"] += m
        if m < n:
            missing[k[0]].append((path, k[2], k[1], ref_lines[k]))
    for k, n in ds.items():
        stats[k[0]]["ds"] += n
        if ref.get(k, 0) < n:
            extra[k[0]].append((path, k[2], k[1], ds_lines[k]))

lines = []
def say(text=""):
    lines.append(text)
    print(text)

say(f"## H. Differential check against the official Dart parser (package:analyzer 6.11.0)")
say(f"files listed={len(files)} compared={compared_files} skipped(syntax errors in reference)={sum(1 for v in syntax_errors.values() if v)} "
    f"dartscope_failures={len(ds_failures)} reference_errors={len(ref_errors)} total_dartscope_time={t_total:.1f}s")
say(f"{'kind':14s} {'ref':>6s} {'dartscope':>9s} {'matched':>8s} {'recall':>7s} {'precision':>9s}")
for kind in sorted(stats, key=lambda k: -stats[k]["ref"]):
    s = stats[kind]
    recall = s["match"] / s["ref"] if s["ref"] else float("nan")
    precision = s["match"] / s["ds"] if s["ds"] else float("nan")
    say(f"{kind:14s} {s['ref']:6d} {s['ds']:9d} {s['match']:8d} {recall:7.1%} {precision:9.1%}")
say()
for kind in ("method", "class", "function", "constructor", "field", "getter", "setter", "variable", "operator", "enum_constant", "import", "export", "part", "typedef", "extension", "mixin", "enum"):
    if missing.get(kind):
        say(f"MISSING {kind} ({len(missing[kind])}): " + "; ".join(f"{os.path.basename(p)}:{l} {o + '.' if o else ''}{n}" for p, o, n, l in missing[kind][:MAX_EXAMPLES]))
for kind in ("method", "class", "function", "constructor", "field", "getter", "setter", "variable", "operator", "import", "export", "part", "typedef", "extension", "mixin", "enum"):
    if extra.get(kind):
        say(f"EXTRA   {kind} ({len(extra[kind])}): " + "; ".join(f"{os.path.basename(p)}:{l} {o + '.' if o else ''}{n}" for p, o, n, l in extra[kind][:MAX_EXAMPLES]))
if ds_failures:
    say("DARTSCOPE FAILURES: " + "; ".join(f"{os.path.basename(p)} exit={c} {e!r}" for p, c, e in ds_failures[:6]))
if slow:
    say("SLOW FILES (>2s): " + "; ".join(f"{t}s {os.path.basename(p)}" for t, p in sorted(slow, reverse=True)[:6]))
if ref_errors:
    say("REFERENCE ERRORS: " + "; ".join(f"{os.path.basename(p)} {e[:80]!r}" for p, e in list(ref_errors.items())[:4]))
open(os.path.join(OUT, "diff_report.txt"), "w").write("\n".join(lines) + "\n")
