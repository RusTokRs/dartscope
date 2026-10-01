#!/usr/bin/env python3
"""TEMPORARY development-loop measurements (removed before hand-off).

Times `dartscope analyze-file` on synthetic inputs at n and 4n, so a linear implementation shows a
ratio near 4 and a quadratic one near 16, and validates every reported span against the source.
"""
import json
import os
import subprocess
import sys
import tempfile
import time

ROOT = os.getcwd()
BIN = os.environ.get("DS_BIN", os.path.join(ROOT, "target", "release", "dartscope"))
TMP = tempfile.mkdtemp(prefix="loop-perf-")
TIMEOUT = int(os.environ.get("PERF_TIMEOUT", "100"))
OUT = []


def say(text=""):
    OUT.append(text)
    print(text, flush=True)


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(data if isinstance(data, bytes) else data.encode("utf-8"))


def run_cli(args, cwd, timeout=TIMEOUT):
    started = time.time()
    try:
        done = subprocess.run([BIN, *args], cwd=cwd, capture_output=True, timeout=timeout)
        return time.time() - started, done.returncode, done.stdout, done.stderr
    except subprocess.TimeoutExpired:
        return float(timeout), None, b"", b""


# ---- generators: (name, function n -> source text, sizes)
def classes(n):
    return "".join(
        f"class C{i} {{\n  final int f{i};\n  C{i}(this.f{i});\n  int m{i}(int a) => a + f{i};\n}}\n"
        for i in range(n)
    )


def functions(n):
    return "".join(f"int f{i}(int a) => a + {i};\n" for i in range(n))


def list_literal(n):
    return "const xs = [\n" + "".join(f"  {i},\n" for i in range(n)) + "];\n"


def map_literal(n):
    return "const m = {\n" + "".join(f"  'k{i}': {i},\n" for i in range(n)) + "};\n"


def enum_constants(n):
    return "enum E {\n" + "".join(f"  c{i},\n" for i in range(n)) + "  ;\n  const E();\n}\n"


def call_arguments(n):
    return "void f() {\n  g(\n" + "".join(f"    {i},\n" for i in range(n)) + "  );\n}\n"


def one_long_line(n):
    return "const xs = [" + ",".join(str(i) for i in range(n)) + "];\n"


def deep_parentheses(n):
    return "int f() => " + "(" * n + "1" + ")" * n + ";\n"


def deep_blocks(n):
    return "void f() {\n" + "{" * n + "}" * n + "\n}\n"


def many_imports(n):
    return "".join(f"import 'package:a/a{i}.dart';\n" for i in range(n))


def many_references(n):
    body = "".join(f"    this.m();\n    value = value + {i};\n" for i in range(n))
    return f"class A {{\n  int value = 0;\n  void m() {{}}\n  void run() {{\n{body}  }}\n}}\n"


def many_locals(n):
    body = "".join(f"    var v{i} = {i};\n" for i in range(n))
    return f"void run() {{\n{body}}}\n"


def interpolation(n):
    return "".join(
        f"final s{i} = 'a ${{b}} c ${{d.replaceAll(\"'\", '')}} e';\n" for i in range(n)
    )


def crlf_bom_unicode(n):
    text = "".join(
        f"// Комментарий {i} 😀\r\nclass К{i} {{\r\n  int m{i}() => {i};\r\n}}\r\n" for i in range(n)
    )
    return "\ufeff" + text


def long_line_calls(n):
    return "void f() {" + "g();" * n + "}\n"


def long_line_members(n):
    return "class A {" + "".join(f"int m{i}() => {i};" for i in range(n)) + "}\n"


def long_line_classes(n):
    return "".join(f"class C{i} {{}} " for i in range(n)) + "\n"


def long_line_unicode(n):
    return "// \U0001F600 \u043f\u0440\u0438\u0432\u0435\u0442\nvoid f() { var s = '\u043c\u0438\u0440'; " + "g();" * n + "}\n"


def functions_with_bodies(n):
    return "".join(f"void f{i}() {{\n  var x = {i};\n  g(x);\n}}\n" for i in range(n))


def methods_with_bodies(n):
    body = "".join(f"  int m{i}(int a) {{\n    var b = a + {i};\n    return b;\n  }}\n" for i in range(n))
    return "class A {\n" + body + "}\n"


SCENARIOS = [
    ("classes", classes, (2000, 8000, 16000)),
    ("functions", functions, (20000, 80000)),
    ("functions_with_bodies", functions_with_bodies, (5000, 20000)),
    ("methods_with_bodies", methods_with_bodies, (5000, 20000)),
    ("list_literal_lines", list_literal, (50000, 200000)),
    ("map_literal_lines", map_literal, (50000, 200000)),
    ("enum_constants", enum_constants, (20000, 80000)),
    ("call_argument_lines", call_arguments, (50000, 200000)),
    ("one_long_line", one_long_line, (100000, 400000)),
    ("deep_parentheses", deep_parentheses, (5000, 20000)),
    ("deep_blocks", deep_blocks, (2000, 8000)),
    ("many_imports", many_imports, (5000, 20000)),
    ("many_references", many_references, (10000, 40000)),
    ("many_locals", many_locals, (20000, 80000)),
    ("interpolation", interpolation, (20000, 80000)),
    ("crlf_bom_unicode", crlf_bom_unicode, (8000, 32000)),
    ("long_line_calls", long_line_calls, (20000, 80000)),
    ("long_line_members", long_line_members, (5000, 20000)),
    ("long_line_classes", long_line_classes, (5000, 20000)),
    ("long_line_unicode", long_line_unicode, (20000, 80000)),
]


# ---- span oracle: every reported span must agree with the source (BOM is a preamble)
def expected_line_col(src, offset):
    preamble = 3 if src.startswith(b"\xef\xbb\xbf") else 0
    line = src.count(b"\n", preamble, offset) + 1
    line_start = max(src.rfind(b"\n", 0, offset) + 1, preamble)
    return line, len(src[line_start:offset].decode("utf-8", "replace")) + 1


def spans_of(node, found):
    if isinstance(node, dict):
        if "byte_start" in node and "byte_end" in node and "start_line" in node:
            found.append(node)
        for value in node.values():
            spans_of(value, found)
    elif isinstance(node, list):
        for value in node:
            spans_of(value, found)


def span_problems(src, data):
    spans = []
    spans_of(data, spans)
    bad = []
    for span in spans:
        start, end = span["byte_start"], span["byte_end"]
        if not (0 <= start <= end <= len(src)):
            bad.append(f"range {start}..{end}")
            continue
        want = (expected_line_col(src, start), expected_line_col(src, end))
        got = (
            (span["start_line"], span["start_column"]),
            (span["end_line"], span["end_column"]),
        )
        if want != got:
            snippet = src[start : start + 20].decode("utf-8", "replace")
            bad.append(f"@{start}..{end} want {want} got {got} near {snippet!r}")
    return len(spans), bad


def scaling():
    say("== analyze-file scaling (seconds; ratio = time(4n)/time(n), ~4 linear, ~16 quadratic) ==")
    for name, generate, sizes in SCENARIOS:
        previous = None
        row = []
        for n in sizes:
            source = generate(n)
            path = os.path.join(TMP, f"{name}-{n}", "lib", "a.dart")
            write(path, source)
            secs, code, out, err = run_cli(["analyze-file", "lib/a.dart"], os.path.dirname(os.path.dirname(path)))
            note = ""
            if code is None:
                note = " TIMEOUT"
            elif code != 0:
                note = f" exit={code} {err.decode('utf-8', 'replace')[:80]!r}"
            ratio = ""
            if previous and previous[1] > 0.05 and sizes[sizes.index(n) - 1] * 4 == n:
                ratio = f" x{secs / previous[1]:.1f}"
            row.append(f"n={n} ({len(source) / 1e6:.2f}MB) {secs:.2f}s{ratio}{note}")
            previous = (n, secs)
        say(f"{name}: " + " | ".join(row))


def spans():
    say("== span oracle on mixed line endings, BOM and non-ASCII text ==")
    samples = {
        "lf": "class A {\n  int x = 1;\n  void m() { var y = x; }\n}\n",
        "crlf": "class A {\r\n  int x = 1;\r\n  void m() { var y = x; }\r\n}\r\n",
        "bom": "\ufeffclass A {\n  int x = 1;\n}\nclass B {}\n",
        "bom_crlf": "\ufeffclass A {\r\n  int x = 1;\r\n}\r\nclass B {}\r\n",
        "unicode": "// Привет 😀\nclass К {\n  String s = 'мир 😀';\n  void m() { var 😀y = 1; }\n}\nclass B {}\n",
        "no_final_newline": "class A {}\nclass B {}",
        "blank_lines": "\n\n\nclass A {\n\n  int x;\n\n}\n\n",
        "enum": "enum E {\n  a,\n  b(1),\n  c;\n  const E([int v = 0]);\n  int get g => 1;\n}\n",
        "accessors": "int get total => 1;\nset total(int v) {}\nT first<T>(List<T> xs) => xs.first;\n",
        "interpolation": "final s = 'a ${b.replaceAll(\"'\", '')} c';\nclass A {}\n",
    }
    for name, text in samples.items():
        src = text.encode("utf-8")
        path = os.path.join(TMP, f"span-{name}", "lib", "a.dart")
        write(path, src)
        secs, code, out, err = run_cli(["analyze-file", "lib/a.dart"], os.path.dirname(os.path.dirname(path)), 30)
        if code != 0:
            say(f"{name}: exit={code} {err.decode('utf-8', 'replace')[:100]!r}")
            continue
        data = json.loads(out.decode("utf-8")).get("data", {})
        total, bad = span_problems(src, data)
        names = [f"{d['kind']}:{d['name']}" for d in data.get("declarations", [])]
        say(f"{name}: {total} spans, {len(bad)} bad {bad[:2]} decls={names}")


def real_files():
    root = os.environ.get("PERF_CORPUS")
    if not root or not os.path.isdir(root):
        return
    say("== real files (largest first) ==")
    files = []
    for base, _dirs, names in os.walk(root):
        for name in names:
            if name.endswith(".dart"):
                full = os.path.join(base, name)
                files.append((os.path.getsize(full), full))
    files.sort(reverse=True)
    for size, full in files[:6]:
        secs, code, out, err = run_cli(["analyze-file", os.path.relpath(full, root)], root)
        say(f"{os.path.relpath(full, root)} {size / 1e6:.2f}MB -> {secs:.2f}s exit={code}")
    started = time.time()
    secs, code, out, err = run_cli(["analyze-project", "."], root, 300)
    say(f"analyze-project over {len(files)} files -> {secs:.2f}s exit={code}")


if __name__ == "__main__":
    which = sys.argv[1:] or ["spans", "scaling", "real"]
    if "spans" in which:
        spans()
    if "scaling" in which:
        scaling()
    if "real" in which:
        real_files()
