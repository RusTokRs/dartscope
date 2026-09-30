#!/usr/bin/env python3
"""Runtime probe for the dartscope CLI (engineering audit only; not part of the product).

Usage: runtime.py SECTION   where SECTION in {battery, projects, perf, cli, fuzz, corpus}
Writes a compact text report to $PROBE_OUT/runtime_<section>.txt and prints it.
"""
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.getcwd()
EXE = ".exe" if sys.platform == "win32" else ""
BIN = os.environ.get("DS_BIN", os.path.join(ROOT, "target", "release", "dartscope" + EXE))
OUT = os.environ.get("PROBE_OUT", tempfile.gettempdir())
TMP = tempfile.mkdtemp(prefix="dsprobe-")
IS_MAC = sys.platform == "darwin"
LINES = []


def say(text=""):
    LINES.append(text)
    print(text, flush=True)


# --------------------------------------------------------------------------- process helpers
def run(args, cwd=None, timeout=60, env=None):
    """Run the CLI once; report exit code (negative = signal), seconds, peak RSS and output."""
    with tempfile.TemporaryFile() as out_file, tempfile.TemporaryFile() as err_file:
        started = time.time()
        proc = subprocess.Popen([BIN, *args], cwd=cwd, stdout=out_file, stderr=err_file, env=env)
        timed_out = False
        rss_mb = 0.0
        while True:
            pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
            if pid:
                break
            if time.time() - started > timeout:
                timed_out = True
                proc.kill()
                pid, status, usage = os.wait4(proc.pid, 0)
                break
            time.sleep(0.002)
        secs = time.time() - started
        code = -os.WTERMSIG(status) if os.WIFSIGNALED(status) else os.WEXITSTATUS(status)
        rss_mb = usage.ru_maxrss / (1024 * 1024 if IS_MAC else 1024)
        out_file.seek(0)
        err_file.seek(0)
        out = out_file.read()
        err = err_file.read()
    return {"code": code, "secs": secs, "rss_mb": rss_mb, "out": out, "err": err, "timeout": timed_out}


def parse_json(result):
    try:
        return json.loads(result["out"].decode("utf-8", "replace"))
    except Exception:
        return None


def first_line(blob, limit=160):
    text = blob.decode("utf-8", "replace").strip().splitlines()
    return (text[0][:limit] if text else "")


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(data if isinstance(data, bytes) else data.encode("utf-8"))


# --------------------------------------------------------------------------- span oracle
def expected_line_col(src, offset):
    line = src.count(b"\n", 0, offset) + 1
    line_start = src.rfind(b"\n", 0, offset) + 1
    return line, len(src[line_start:offset].decode("utf-8", "replace")) + 1


def walk_spans(node, found):
    if isinstance(node, dict):
        if "byte_start" in node and "byte_end" in node and "start_line" in node:
            found.append(node)
        for value in node.values():
            walk_spans(value, found)
    elif isinstance(node, list):
        for value in node:
            walk_spans(value, found)


def span_problems(src, data, limit=3):
    """Validate every span against the source. Returns (checked, bad_count, examples)."""
    spans = []
    walk_spans(data, spans)
    bad = 0
    examples = []
    for span in spans:
        start, end = span["byte_start"], span["byte_end"]
        why = None
        if not (0 <= start <= end <= len(src)):
            why = "out of range"
        elif (start < len(src) and 0x80 <= src[start] <= 0xBF) or (
            end < len(src) and 0x80 <= src[end] <= 0xBF
        ):
            why = "not on a char boundary"
        else:
            want_start = expected_line_col(src, start)
            want_end = expected_line_col(src, end)
            got_start = (span["start_line"], span["start_column"])
            got_end = (span["end_line"], span["end_column"])
            if want_start != got_start:
                why = f"start line/col {got_start} != expected {want_start}"
            elif want_end != got_end:
                why = f"end line/col {got_end} != expected {want_end}"
        if why:
            bad += 1
            if len(examples) < limit:
                snippet = src[max(0, start) : min(len(src), start + 24)].decode("utf-8", "replace")
                examples.append(f"{why} @{start}..{end} near {snippet!r}")
    return len(spans), bad, examples


def facts(doc, src):
    data = (doc or {}).get("data") or {}
    decls = [f"{d['kind']}:{d['name'][:40]}" for d in data.get("declarations", [])]
    checked, bad, examples = span_problems(src, data)
    return {
        "decls": decls,
        "imports": [i["uri"] for i in data.get("imports", [])],
        "exports": [e["uri"] for e in data.get("exports", [])],
        "parts": [p["uri"] for p in data.get("parts", [])],
        "diags": [d["code"] for d in data.get("diagnostics", [])],
        "consts": [c["name"] for c in data.get("string_constants", [])],
        "invocations": len(data.get("invocations", [])),
        "widgets": [w["class_name"] for w in data.get("flutter", {}).get("widgets", [])],
        "spans": f"{checked} checked / {bad} bad",
        "span_examples": examples,
    }


def analyze_source(source, name="lib/a.dart", timeout=30):
    base = os.path.join(TMP, f"case-{random.randrange(10**9)}")
    src_bytes = source if isinstance(source, bytes) else source.encode("utf-8")
    write(os.path.join(base, name), src_bytes)
    result = run(["analyze-file", name], cwd=base, timeout=timeout)
    shutil.rmtree(base, ignore_errors=True)
    return result, src_bytes


def show_case(case_id, description, source, expect=None, timeout=30):
    result, src = analyze_source(source, timeout=timeout)
    doc = parse_json(result)
    head = f"[{case_id}] {description}: exit={result['code']} {result['secs']*1000:.0f}ms"
    if result["timeout"]:
        head += " TIMEOUT"
    if doc is None:
        say(f"{head} stderr={first_line(result['err'])!r}")
        return result, None
    info = facts(doc, src)
    say(head)
    say(f"    decls={info['decls'][:14]}{'…' if len(info['decls']) > 14 else ''}")
    extras = {k: v for k, v in info.items() if k in ("imports", "exports", "parts", "consts", "widgets") and v}
    if extras:
        say(f"    {extras}")
    say(f"    diags={info['diags'][:8]} invocations={info['invocations']} spans={info['spans']}")
    for example in info["span_examples"]:
        say(f"    SPAN: {example}")
    if expect:
        missing = [item for item in expect if item not in info["decls"]]
        if missing:
            say(f"    !! EXPECTED BUT MISSING: {missing}")
    return result, info


# --------------------------------------------------------------------------- sections
def section_battery():
    say("## A. Dart source battery (analyze-file)")
    show_case("A01", "control, LF", "import 'package:a/a.dart';\nclass A {}\nclass B {}\n", ["class:A", "class:B"])
    show_case("A02", "UTF-8 BOM before first directive", "\ufeffimport 'package:a/a.dart';\nclass A {}\n", ["class:A"])
    show_case("A03", "UTF-8 BOM before first class", "\ufeffclass First {}\nclass Second {}\n", ["class:First", "class:Second"])
    show_case("A04", "CRLF", "import 'a.dart';\r\nclass A {}\r\nclass B {}\r\n", ["class:A", "class:B"])
    show_case("A05", "lone CR line endings", "import 'a.dart';\rclass A {}\rclass B {}\r", ["class:A", "class:B"])
    show_case("A06", "interpolation with nested same-type quotes",
              "const s = 'a ${b['c']} d';\nclass After {}\nclass After2 {}\n", ["class:After", "class:After2"])
    show_case("A07", "interpolation with quote inside inner string",
              "final t = '${x.replaceAll(\"'\", '')}';\nclass After {}\nclass After2 {}\n", ["class:After", "class:After2"])
    show_case("A08", "class keyword inside string/comment",
              "// class Fake {}\nconst s = 'class AlsoFake {}';\n/* class Fake3 {} */\nclass Real {}\n", ["class:Real"])
    show_case("A09", "non-ASCII comments/strings before declarations",
              "// Привет, мир 😀\nconst greeting = 'Привет 日本語 😀';\n/* é ü */\nclass Ünï {}\nclass Real {}\n", ["class:Real"])
    show_case("A10", "unterminated string then class",
              "const s = 'oops;\nclass After {}\n", ["class:After"])
    show_case("A11", "unterminated block comment", "class Before {}\n/* never closed\nclass Inside {}\n", ["class:Before"])
    show_case("A12", "enum with constants", "enum Color { red, green, blue }\nenum E2 { a(1), b(2); const E2(this.v); final int v; }\n",
              ["enum:Color", "enum:E2"])
    show_case("A13", "top-level getter/setter/function/variable",
              "int get total => 1;\nset total(int v) {}\nint compute() => 2;\nfinal answer = 42;\nconst kPad = 8.0;\nlate int lazy;\n")
    show_case("A14", "Dart 3 class modifiers",
              "sealed class S {}\nbase class B {}\ninterface class I {}\nfinal class F {}\nmixin class M {}\nabstract interface class AI {}\nabstract base class AB {}\n",
              ["class:S", "class:B", "class:I", "class:F", "class:M", "class:AI", "class:AB"])
    show_case("A15", "extension types, typedefs, extensions",
              "extension type ET(int i) {}\ntypedef R = (int, String);\ntypedef Cb = void Function(int);\nextension StrX on String { int get n => 1; }\nextension on int {}\n",
              ["extension_type:ET", "typedef:R", "typedef:Cb", "extension:StrX"])
    show_case("A16", "records, patterns, switch expressions",
              "(int, int) pair() => (1, 2);\nString f(Object o) => switch (o) { int() => 'i', _ => 'x' };\nclass Z {}\n", ["class:Z"])
    show_case("A17", "annotations (stacked, multiline, generic, dollar)",
              "@immutable\n@JsonSerializable(explicitToJson: true)\nclass Ann {}\n@_$Gen<int>(a: 1)\nclass Ann2 {}\n@Deprecated('x')\nvoid old() {}\n",
              ["class:Ann", "class:Ann2", "function:old"])
    show_case("A18", "generic functions, where clauses, factories, redirecting ctors",
              "class G<T extends Object> {\n  G();\n  factory G.named() = G;\n  G.other() : this();\n  static const int k = 1;\n  external void ext();\n  late final int v;\n  Map<String, List<int>> m<U>(U a) => {};\n}\n", ["class:G"])
    show_case("A19", "Flutter widget + routes",
              "import 'package:flutter/material.dart';\nclass MyApp extends StatelessWidget {\n  Widget build(BuildContext c) => MaterialApp(routes: {'/': (c) => Home()}, home: Home());\n}\nclass Home extends StatefulWidget { State createState() => _S(); }\n",
              ["class:MyApp", "class:Home"])
    show_case("A20", "part/part of/library directives",
              "library foo.bar;\nimport 'a.dart' as a show X hide Y;\nexport 'b.dart';\npart 'c.dart';\nclass L {}\n", ["class:L"])
    show_case("A21", "conditional imports",
              "import 'stub.dart'\n  if (dart.library.io) 'io.dart'\n  if (dart.library.js_interop) 'web.dart' show Api;\nclass C {}\n", ["class:C"])
    show_case("A22", "raw & triple-quoted strings with quotes/braces",
              "const a = r'{ class NotReal {} }';\nconst b = '''\nclass AlsoNot {}\n''';\nconst c = \"\"\"}\"\"\";\nclass Real2 {}\n", ["class:Real2"])
    show_case("A23", "dollar identifiers", "class $Gen {}\nclass _$Private {}\nvoid jni$_() {}\nclass Tail {}\n", ["class:Tail"])
    show_case("A24", "NUL and control chars", "class A {}\x00\x01\nclass B {}\n", ["class:A", "class:B"])
    show_case("A25", "U+2028/U+2029/NEL separators", "class A {}\u2028class B {}\u2029class C {}\u0085class D {}\n")
    show_case("A26", "merge conflict markers", "<<<<<<< HEAD\nclass A {}\n=======\nclass B {}\n>>>>>>> branch\n")
    show_case("A27", "shebang", "#!/usr/bin/env dart\nvoid main() {}\nclass A {}\n", ["class:A"])
    show_case("A28", "empty file", "")
    show_case("A29", "only whitespace/newlines", "\n\n   \n\t\n")
    show_case("A30", "unicode escapes / emoji in identifiers-adjacent positions",
              "const s = '\\u{1F600}';\nconst t = 'a\u200db';\nclass AfterEmoji {}\n", ["class:AfterEmoji"])
    say()
    say("## A2. Nesting / size stress (exit code must be 0; anything else is a crash)")
    for label, source in (
        ("parens x5000", "void f() { " + "(" * 5000 + ")" * 5000 + "; }\nclass Z {}\n"),
        ("parens x100000", "void f() { " + "(" * 100000 + ")" * 100000 + "; }\nclass Z {}\n"),
        ("brackets x100000", "var x = " + "[" * 100000 + "]" * 100000 + ";\nclass Z {}\n"),
        ("braces x100000", "void f() " + "{" * 100000 + "}" * 100000 + "\nclass Z {}\n"),
        ("angles x100000", "class A<" + "T<" * 100000 + "int" + ">" * 100000 + "> {}\nclass Z {}\n"),
        ("unclosed parens x200000", "void f() { " + "(" * 200000 + "\nclass Z {}\n"),
        ("unclosed annotation parens", "@A(" * 20000 + "\nclass Z {}\n"),
        ("nested ternary x20000", "var x = " + "a ? b : " * 20000 + "c;\nclass Z {}\n"),
        ("nested strings x5000", "var s = " + "'${" * 5000 + "1" + "}'" * 5000 + ";\nclass Z {}\n"),
        ("1 MB single line", "const x = " + " + ".join(["1"] * 250000) + ";\nclass Z {}\n"),
        ("100k blank lines + class", "\n" * 100000 + "class Z {}\n"),
        ("10k-char identifier", "class " + "A" * 10000 + " {}\n"),
    ):
        show_case("N", label, source, timeout=60)


def section_perf():
    say("## B. Scaling (analyze-file): time should grow ~linearly with size")
    for count in (500, 2000, 8000, 16000):
        body = "".join(
            f"class C{i} {{\n  final int f{i};\n  C{i}(this.f{i});\n  int m{i}(int a) => a + f{i};\n}}\n" for i in range(count)
        )
        result, _ = analyze_source(body, timeout=150)
        say(f"classes={count:6d} lines={count*5:7d} bytes={len(body):9d}: exit={result['code']} "
            f"{result['secs']:.2f}s rss={result['rss_mb']:.0f}MB" + (" TIMEOUT" if result["timeout"] else ""))
    for count in (2000, 8000, 32000):
        body = "void main() {\n" + "".join(f"  f{i}(a{i}, b{i});\n" for i in range(count)) + "}\n"
        result, _ = analyze_source(body, timeout=240)
        say(f"calls={count:6d}: exit={result['code']} {result['secs']:.2f}s rss={result['rss_mb']:.0f}MB"
            + (" TIMEOUT" if result["timeout"] else ""))
    say()
    say("## B2. Project-level scaling (analyze-project and lint with all rules)")
    cfg = os.path.join(TMP, "all-rules.toml")
    write(cfg, 'version = 1\nenabled_rules = ["dartscope.forbidden_import","dartscope.layer_boundary","dartscope.naming_convention","dartscope.unresolved_part","dartscope.orphan_file"]\n[orphan_files]\nentry_points = ["lib/main.dart"]\n')
    for files in (100, 400, 1600):
        proj = os.path.join(TMP, f"proj-{files}")
        write(os.path.join(proj, "pubspec.yaml"), "name: demo\nenvironment:\n  sdk: ^3.0.0\n")
        write(os.path.join(proj, "lib", "main.dart"), "".join(f"import 'src/f{i}.dart';\n" for i in range(files)) + "void main() {}\n")
        for i in range(files):
            write(os.path.join(proj, "lib", "src", f"f{i}.dart"),
                  f"import 'f{(i + 1) % files}.dart';\nclass F{i} {{\n  void run() {{ F{(i + 1) % files}().run(); }}\n}}\n")
        for command in (["analyze-project", proj], ["lint", proj, "--config", cfg]):
            result = run(command, timeout=240)
            say(f"files={files:5d} {command[0]:16s}: exit={result['code']} {result['secs']:.2f}s rss={result['rss_mb']:.0f}MB out={len(result['out'])//1024}KB"
                + (" TIMEOUT" if result["timeout"] else "") + (f" err={first_line(result['err'])!r}" if result["code"] not in (0, 4) else ""))
        shutil.rmtree(proj, ignore_errors=True)


def section_projects():
    say("## C. Project traversal and input handling")
    # C1 directory symlink pointing outside the root (Flutter: ios/.symlinks/plugins/*, windows/flutter/ephemeral/.plugin_symlinks/*)
    external = os.path.join(TMP, "external-pkg")
    write(os.path.join(external, "lib", "x.dart"), "class External {}\n")
    proj = os.path.join(TMP, "flutter-like")
    write(os.path.join(proj, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    write(os.path.join(proj, "lib", "main.dart"), "void main() {}\n")
    os.makedirs(os.path.join(proj, "ios", ".symlinks", "plugins"), exist_ok=True)
    os.symlink(external, os.path.join(proj, "ios", ".symlinks", "plugins", "some_plugin"))
    result = run(["analyze-project", proj])
    say(f"[C01] Flutter-style ios/.symlinks/plugins/<dir symlink outside root>: exit={result['code']} err={first_line(result['err'])!r}")
    result = run(["lint", proj])
    say(f"[C01b] same tree, lint: exit={result['code']} err={first_line(result['err'])!r}")
    # C2 symlink to a file inside the root
    proj = os.path.join(TMP, "file-symlink")
    write(os.path.join(proj, "lib", "real.dart"), "class Real {}\n")
    os.symlink("real.dart", os.path.join(proj, "lib", "alias.dart"))
    result = run(["analyze-project", proj])
    doc = parse_json(result)
    paths = [f["path"] for f in doc["data"]["files"]] if doc else None
    say(f"[C02] in-root file symlink: exit={result['code']} files={paths}")
    # C3 non-UTF-8 source
    proj = os.path.join(TMP, "non-utf8")
    write(os.path.join(proj, "lib", "good.dart"), "class Good {}\n")
    write(os.path.join(proj, "lib", "bad.dart"), b"// \xd0\xcf\xd0\xd0\xd2 cp1251\nclass Bad {}\n")
    result = run(["analyze-project", proj])
    say(f"[C03] one non-UTF-8 .dart file next to a good one: exit={result['code']} err={first_line(result['err'])!r}")
    # C4 directories named like generated dirs (build/target/coverage/Pods) used as real source dirs
    proj = os.path.join(TMP, "named-dirs")
    for d in ("build", "target", "coverage", "Pods", "node_modules"):
        write(os.path.join(proj, "lib", "src", d, "x.dart"), f"class In{d.capitalize()} {{}}\n")
    write(os.path.join(proj, "lib", "main.dart"), "void main() {}\n")
    result = run(["analyze-project", proj])
    doc = parse_json(result)
    say(f"[C04] lib/src/{{build,target,coverage,Pods,node_modules}}/x.dart silently skipped? files={[f['path'] for f in doc['data']['files']] if doc else None}")
    # C5 non-ASCII and spaced directory names
    proj = os.path.join(TMP, "проект с пробелом")
    write(os.path.join(proj, "lib", "файл имя.dart"), "class Ok {}\n")
    write(os.path.join(proj, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    result = run(["analyze-project", proj])
    doc = parse_json(result)
    say(f"[C05] non-ASCII dir + file name with space: exit={result['code']} files={[f['path'] for f in doc['data']['files']] if doc else first_line(result['err'])} root={doc['data']['root'] if doc else None!r}")
    # C6 determinism + absolute paths in output
    proj = os.path.join(TMP, "determinism")
    write(os.path.join(proj, "lib", "a.dart"), "import 'b.dart';\nclass A {}\n")
    write(os.path.join(proj, "lib", "b.dart"), "class B {}\n")
    write(os.path.join(proj, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    first = run(["analyze-project", proj])
    second = run(["analyze-project", proj])
    rel = run(["analyze-project", "."], cwd=proj)
    doc_abs, doc_rel = parse_json(first), parse_json(rel)
    say(f"[C06] deterministic across runs: {first['out'] == second['out']}; output contains machine-absolute root: {doc_abs['data']['root']!r}; "
        f"same project via '.' -> root={doc_rel['data']['root']!r}; identical output abs-vs-rel: {first['out'] == rel['out']}")
    # C7 missing/invalid inputs: exit codes
    for label, args in (
        ("nonexistent path", ["analyze-project", os.path.join(TMP, "nope")]),
        ("file given as project root", ["analyze-project", os.path.join(proj, "lib", "a.dart")]),
        ("directory given to analyze-file", ["analyze-file", proj]),
        ("empty directory", ["analyze-project", os.path.join(TMP, "empty-dir")]),
        ("unknown command", ["frobnicate", "x"]),
        ("no args", []),
        ("--version", ["--version"]),
        ("lint bad format", ["lint", proj, "--format", "xml"]),
        ("lint missing config file", ["lint", proj, "--config", os.path.join(TMP, "missing.toml")]),
    ):
        if label == "empty directory":
            os.makedirs(os.path.join(TMP, "empty-dir"), exist_ok=True)
        result = run(args)
        say(f"[C07] {label}: exit={result['code']} err={first_line(result['err'], 110)!r}")
    # C8 pubspec / package_config / lint configuration robustness
    proj = os.path.join(TMP, "yaml-edge")
    write(os.path.join(proj, "lib", "main.dart"), "import 'package:dep/dep.dart';\nvoid main() {}\n")
    for label, yaml in (
        ("tabs", "name: app\nenvironment:\n\tsdk: ^3.0.0\n"),
        ("anchors+merge", "name: app\nbase: &b {sdk: ^3.0.0}\nenvironment:\n  <<: *b\n"),
        ("alias bomb", "a: &a [x,x,x,x,x,x,x,x,x,x]\nb: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a,*a]\nc: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b,*b]\nd: &d [*c,*c,*c,*c,*c,*c,*c,*c,*c,*c]\ne: &e [*d,*d,*d,*d,*d,*d,*d,*d,*d,*d]\nf: &f [*e,*e,*e,*e,*e,*e,*e,*e,*e,*e]\nname: app\n"),
        ("duplicate keys", "name: a\nname: b\ndependencies:\n  x: 1.0.0\n  x: 2.0.0\n"),
        ("deps odd shapes", "name: app\ndependencies:\n  a: ^1.0.0\n  b: {path: ../b}\n  c: {git: {url: 'https://x/y.git', ref: main, path: pkg}}\n  d: {hosted: {name: d2, url: 'https://p.example'}, version: '>=1.0.0 <2.0.0'}\n  e:\n  f: any\n  flutter: {sdk: flutter}\n"),
        ("huge numbers/odd scalars", "name: 123456789012345678901234567890\nversion: 1.0.0+99999999999999999999\nenvironment: {sdk: '>=3.0.0 <4.0.0', flutter: null}\n"),
        ("flutter assets/fonts", "name: app\nflutter:\n  uses-material-design: true\n  assets:\n    - assets/\n    - path: assets/x.png\n      flavors: [a]\n      transformers: [{package: t, args: ['--x']}]\n  fonts:\n    - family: F\n      fonts:\n        - asset: fonts/F.ttf\n          weight: 700\n"),
        ("BOM + CRLF", "\ufeffname: app\r\nenvironment:\r\n  sdk: ^3.0.0\r\n"),
        ("not yaml", "{{project_name}}: [unclosed\n  - : :\n"),
        ("empty", ""),
    ):
        write(os.path.join(proj, "pubspec.yaml"), yaml)
        for command in ("pubspec", "pubspec-config"):
            result = run([command, os.path.join(proj, "pubspec.yaml")], timeout=20)
            doc = parse_json(result)
            codes = [d["code"] for d in doc["data"].get("diagnostics", [])] if doc else None
            say(f"[C08] pubspec[{label}] {command}: exit={result['code']} {result['secs']*1000:.0f}ms diags={codes}"
                + ("" if doc else f" err={first_line(result['err'], 90)!r}"))
    proj2 = os.path.join(TMP, "pkgcfg")
    write(os.path.join(proj2, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    write(os.path.join(proj2, "lib", "main.dart"), "import 'package:dep/dep.dart';\nimport 'package:app/util.dart';\nvoid main() {}\n")
    write(os.path.join(proj2, "lib", "util.dart"), "class U {}\n")
    write(os.path.join(proj2, "dep", "lib", "dep.dart"), "class Dep {}\n")
    for label, config in (
        ("valid relative", '{"configVersion":2,"packages":[{"name":"app","rootUri":"../","packageUri":"lib/"},{"name":"dep","rootUri":"../dep","packageUri":"lib/"}]}'),
        ("packageUri without trailing slash", '{"configVersion":2,"packages":[{"name":"app","rootUri":"../","packageUri":"lib"},{"name":"dep","rootUri":"../dep","packageUri":"lib"}]}'),
        ("escaping rootUri", '{"configVersion":2,"packages":[{"name":"dep","rootUri":"../../../../etc","packageUri":"lib/"}]}'),
        ("absolute file rootUri", '{"configVersion":2,"packages":[{"name":"dep","rootUri":"file:///opt/dep","packageUri":"lib/"}]}'),
        ("percent-encoded", '{"configVersion":2,"packages":[{"name":"dep","rootUri":"../d%65p","packageUri":"lib/"}]}'),
        ("duplicate package names", '{"configVersion":2,"packages":[{"name":"dep","rootUri":"../dep","packageUri":"lib/"},{"name":"dep","rootUri":"../other","packageUri":"lib/"}]}'),
        ("wrong configVersion", '{"configVersion":3,"packages":[]}'),
        ("invalid json", '{"configVersion":2,"packages":['),
    ):
        write(os.path.join(proj2, ".dart_tool", "package_config.json"), config)
        result = run(["uri-graph", proj2], timeout=30)
        doc = parse_json(result)
        if doc:
            refs = [(r["uri"], r["resolution"], r.get("target_path")) for r in doc["data"].get("references", [])]
            pcs = doc["data"].get("package_configs")
            say(f"[C09] package_config[{label}]: exit={result['code']} refs={refs}")
        else:
            say(f"[C09] package_config[{label}]: exit={result['code']} err={first_line(result['err'], 120)!r}")


def section_cli():
    say("## D. CLI process behavior")
    proj = os.path.join(TMP, "cli")
    write(os.path.join(proj, "a.dart"), "class A {}\n" * 2000)
    # non-UTF-8 argv
    if sys.platform != "win32":
        result = run(["analyze-file", b"\xff\xfe.dart"])
        say(f"[D01] non-UTF-8 argv: exit={result['code']} (expect 2/3; 101 = panic) stderr={first_line(result['err'], 140)!r}")
    # broken pipe
    big = os.path.join(proj, "a.dart")
    proc = subprocess.Popen([BIN, "analyze-file", big], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    proc.stdout.read(16)
    proc.stdout.close()
    try:
        err = proc.stderr.read()
        code = proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()
        err, code = b"", "TIMEOUT"
    say(f"[D02] stdout closed early (| head): exit={code} stderr={first_line(err, 160)!r}")
    # help/usage exit codes
    for args in (["--help"], ["analyze-file", "--help"], ["analyze-file"], ["analyze-file", big, "extra"], ["help", "lint"], ["lint", proj, "--config"]):
        result = run(args)
        say(f"[D03] args={args[:3]}: exit={result['code']} out={first_line(result['out'], 70)!r} err={first_line(result['err'], 70)!r}")
    # JSON envelope and schema names for every contract
    result = run(["analyze-file", big])
    doc = parse_json(result)
    say(f"[D04] envelope keys={sorted(doc)} schema={doc.get('schema')} version={doc.get('version')}" if doc else "[D04] no JSON")
    # SARIF sanity
    cfg = os.path.join(TMP, "sarif.toml")
    write(cfg, 'version = 1\nenabled_rules = ["dartscope.naming_convention"]\n')
    write(os.path.join(proj, "lib", "BadName.dart"), "class bad_class {}\nvoid BadFn() {}\n")
    result = run(["lint", proj, "--config", cfg, "--format", "sarif"])
    doc = parse_json(result)
    if doc:
        run0 = doc["runs"][0]
        results = run0.get("results", [])
        sample = results[0] if results else {}
        say(f"[D05] SARIF: exit={result['code']} version={doc.get('version')} $schema={doc.get('$schema')!r} results={len(results)} "
            f"rules={[r['id'] for r in run0['tool']['driver'].get('rules', [])]} sample={json.dumps(sample)[:420]}")
    else:
        say(f"[D05] SARIF: exit={result['code']} err={first_line(result['err'])!r} out={first_line(result['out'])!r}")
    # lint with no config: how many rules run?
    result = run(["lint", proj])
    doc = parse_json(result)
    say(f"[D06] lint without --config: exit={result['code']} summary={doc['data']['summary'] if doc else None}")
    # naming rule false positives on generated-style names
    write(os.path.join(proj, "lib", "user.g.dart"),
          "T _$UserFromJson(Map<String, dynamic> j) => throw 0;\nMap<String, dynamic> _$UserToJson(Object o) => {};\nconst int kMaxItems = 3;\nconst int MAX_ITEMS = 4;\nclass _$Foo {}\nclass $Bar {}\nvoid jni$_init() {}\n")
    result = run(["lint", proj, "--config", cfg])
    doc = parse_json(result)
    msgs = sorted({d["message"][:90] for d in doc["data"]["diagnostics"] if d["path"].endswith("user.g.dart")}) if doc else None
    say(f"[D07] naming rule on generated-style names: {msgs}")
    # forbidden import bypass via export / conditional import / relative import
    bypass = os.path.join(TMP, "bypass")
    write(os.path.join(bypass, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    write(os.path.join(bypass, "lib", "ui", "a.dart"), "import 'package:forbidden/x.dart';\n")
    write(os.path.join(bypass, "lib", "ui", "b.dart"), "export 'package:forbidden/x.dart';\n")
    write(os.path.join(bypass, "lib", "ui", "c.dart"), "import 'stub.dart' if (dart.library.io) 'package:forbidden/io.dart';\n")
    write(os.path.join(bypass, "lib", "data", "repo.dart"), "class Repo {}\n")
    write(os.path.join(bypass, "lib", "ui", "d.dart"), "import '../data/repo.dart';\n")
    write(os.path.join(bypass, "lib", "ui", "e.dart"), "export '../data/repo.dart';\n")
    write(os.path.join(bypass, "lib", "ui", "f.dart"), "import 'package:app/data/repo.dart';\n")
    cfg2 = os.path.join(TMP, "bypass.toml")
    write(cfg2, 'version = 1\nenabled_rules = ["dartscope.forbidden_import","dartscope.layer_boundary"]\n'
                '[[forbidden_imports]]\nuri = "package:forbidden/"\nmatch_kind = "prefix"\n'
                '[[layer_boundaries]]\nsource_prefix = "lib/ui"\ndenied_target_prefixes = ["lib/data"]\n')
    write(os.path.join(bypass, ".dart_tool", "package_config.json"),
          '{"configVersion":2,"packages":[{"name":"app","rootUri":"../","packageUri":"lib/"}]}')
    result = run(["lint", bypass, "--config", cfg2])
    doc = parse_json(result)
    hits = sorted((d["path"], d["rule_id"]) for d in doc["data"]["diagnostics"]) if doc else first_line(result["err"])
    say(f"[D08] lint bypass matrix (a=import forbidden, b=export forbidden, c=conditional forbidden, d=rel import layer, e=rel export layer, f=package import layer): exit={result['code']} flagged={hits}")


def extract_corpus(limit_files=400, max_bytes=60000):
    """Dart sources from any checked-out corpus repositories plus the repo's own fixtures."""
    roots = [p for p in os.environ.get("CORPUS_DIRS", "").split(os.pathsep) if p]
    files = []
    for root in roots:
        for base, _dirs, names in os.walk(root):
            if "/.git" in base:
                continue
            for name in names:
                if name.endswith(".dart"):
                    path = os.path.join(base, name)
                    try:
                        if os.path.getsize(path) <= max_bytes:
                            files.append(path)
                    except OSError:
                        pass
    files.sort()
    random.Random(7).shuffle(files)
    return files[:limit_files]


MUTATION_TOKENS = [
    "'", '"', "\\", "$", "${", "{", "}", "(", ")", "[", "]", "<", ">", "@", "/", "*", "//", "/*", "*/", "\n", "\r\n", "\r", ";", ",", ".",
    "?", "!", "=>", "=", ":", "é", "日", "😀", "\u2028", "\ufeff", "\x00", "r'", "'''", '"""', "class ", "enum ", "extension ", "mixin ",
    "import ", "part ", "@override ", "static ", "factory ", "external ", "late ", "final ", "const ", "abstract ", "sealed ", "typedef ",
    "async ", "await ", " extends ", " implements ", " with ", " on ", "get ", "set ", "operator ", "void ", "=> ", "?.", "..", "...",
]


def mutate(rng, text):
    for _ in range(rng.randint(1, 4)):
        if not text:
            text = rng.choice(MUTATION_TOKENS)
        kind = rng.choice(("trunc", "del", "ins", "dup", "swap", "rep", "ins"))
        pos = rng.randrange(len(text) + 1)
        if kind == "trunc":
            text = text[:pos]
        elif kind == "del":
            text = text[:pos] + text[pos + rng.randint(1, 40):]
        elif kind == "ins":
            text = text[:pos] + rng.choice(MUTATION_TOKENS) + text[pos:]
        elif kind == "dup":
            seg = text[pos : pos + rng.randint(1, 120)]
            text = text[:pos] + seg * rng.randint(2, 6) + text[pos:]
        elif kind == "swap" and len(text) > 4:
            a, b = sorted((rng.randrange(len(text)), rng.randrange(len(text))))
            text = text[:a] + text[b : b + 30] + text[a + 30 : b] + text[a : a + 30] + text[b + 30 :]
        else:
            text = text[:pos] + rng.choice(MUTATION_TOKENS) + text[pos + 1 :]
    return text


def anomalous(result):
    return result["code"] != 0 or result["timeout"] or result["secs"] > 8


def minimize(text, predicate, budget=80):
    """Greedy chunk-deletion minimiser."""
    size = max(1, len(text) // 2)
    tries = 0
    while size >= 1 and tries < budget:
        pos = 0
        shrunk = False
        while pos < len(text) and tries < budget:
            candidate = text[:pos] + text[pos + size :]
            tries += 1
            if candidate != text and predicate(candidate):
                text = candidate
                shrunk = True
            else:
                pos += size
        if not shrunk:
            size //= 2
    return text


def section_fuzz():
    seeds_dir_files = extract_corpus(limit_files=250, max_bytes=30000)
    seeds = []
    for path in seeds_dir_files:
        try:
            seeds.append(open(path, encoding="utf-8").read())
        except (OSError, UnicodeDecodeError):
            pass
    builtin = [
        "import 'package:flutter/material.dart';\npart 'x.g.dart';\n@immutable\nclass A<T extends Object> extends B with C implements D {\n  final int x;\n  A(this.x) : super();\n  factory A.f() => A(1);\n  static const k = {'a': 1, \"b\": [1, 2]};\n  Future<void> m(int a, {required String b}) async { var s = '${a + 1} $b'; if (a > 0) { print(s); } }\n}\nenum E { a, b }\nextension X on String { int get n => length; }\nmixin M {}\ntypedef F = void Function(int);\nconst q = gql(r'''query Q { a { b } }''');\n",
        "void main() { for (var i = 0; i < 3; i++) { final f = (int a) => a * i; switch (i) { case 1: break; default: } try { f(1); } on Exception catch (e) { rethrow; } finally {} } }\n",
    ]
    seeds.extend(builtin)
    say(f"## E. Mutation fuzzing of analyze-file + lint ({len(seeds)} seed files; {sys.platform})")
    rng = random.Random(20260930)
    deadline = time.time() + float(os.environ.get("FUZZ_SECONDS", "150"))
    proj = os.path.join(TMP, "fuzz-proj")
    write(os.path.join(proj, "pubspec.yaml"), "name: app\nenvironment:\n  sdk: ^3.0.0\n")
    cfg = os.path.join(TMP, "fuzz.toml")
    write(cfg, 'version = 1\nenabled_rules = ["dartscope.forbidden_import","dartscope.layer_boundary","dartscope.naming_convention","dartscope.unresolved_part","dartscope.orphan_file"]\n[orphan_files]\nentry_points = ["lib/a.dart"]\n')
    runs = 0
    anomalies = []
    span_bad = []
    signature_seen = set()
    while time.time() < deadline:
        seed = rng.choice(seeds)
        text = mutate(rng, seed)
        target = os.path.join(proj, "lib", "a.dart")
        write(target, text)
        mode = "analyze-file" if runs % 3 else "lint"
        args = ["analyze-file", os.path.join("lib", "a.dart")] if mode == "analyze-file" else ["lint", ".", "--config", cfg]
        result = run(args, cwd=proj, timeout=20)
        runs += 1
        if mode == "analyze-file" and result["code"] == 0:
            doc = parse_json(result)
            if doc:
                checked, bad, examples = span_problems(text.encode("utf-8"), doc.get("data", {}), limit=1)
                if bad and len(span_bad) < 6:
                    span_bad.append((bad, checked, examples[0], text))
        if anomalous(result) and not (mode == "lint" and result["code"] in (4, 6)):
            signature = (mode, result["code"], first_line(result["err"], 80))
            if signature in signature_seen and len(anomalies) >= 4:
                continue
            signature_seen.add(signature)

            def still_bad(candidate, mode=mode, args=args):
                write(target, candidate)
                again = run(args, cwd=proj, timeout=20)
                return anomalous(again) and not (mode == "lint" and again["code"] in (4, 6))

            small = minimize(text, still_bad)
            anomalies.append((mode, result["code"], result["timeout"], round(result["secs"], 1), first_line(result["err"], 200), small))
            if len(anomalies) >= 12:
                break
    say(f"runs={runs} anomalies(non-zero exit/timeout/>8s)={len(anomalies)} span-invariant-violations={len(span_bad)}")
    for mode, code, timeout, secs, err, small in anomalies:
        say(f"ANOMALY mode={mode} exit={code} timeout={timeout} secs={secs} stderr={err!r}")
        say(f"    minimized({len(small)} chars)={small[:400]!r}")
    for bad, checked, example, text in span_bad:
        say(f"SPAN-VIOLATION {bad}/{checked}: {example}")
        say(f"    source({len(text)} chars) head={text[:300]!r}")


def section_corpus():
    roots = [p for p in os.environ.get("CORPUS_DIRS", "").split(os.pathsep) if p]
    say(f"## F. Real-world corpus ({len(roots)} repositories)")
    cfg = os.path.join(TMP, "corpus.toml")
    write(cfg, 'version = 1\nenabled_rules = ["dartscope.forbidden_import","dartscope.layer_boundary","dartscope.naming_convention","dartscope.unresolved_part","dartscope.orphan_file"]\n[orphan_files]\nentry_points = ["lib/main.dart"]\n')
    for root in roots:
        name = os.path.basename(root.rstrip("/"))
        dart_files = sum(1 for _b, _d, fs in os.walk(root) for f in fs if f.endswith(".dart"))
        for command in (["analyze-project", root], ["lint", root, "--config", cfg]):
            result = run(command, timeout=300)
            doc = parse_json(result)
            note = ""
            if doc and command[0] == "analyze-project":
                data = doc["data"]
                codes = {}
                for d in data.get("diagnostics", []):
                    codes[d["code"]] = codes.get(d["code"], 0) + 1
                for f in data.get("files", []):
                    for d in f.get("diagnostics", []):
                        codes[d["code"]] = codes.get(d["code"], 0) + 1
                note = f"summary={data['summary']} diag_codes={dict(sorted(codes.items(), key=lambda kv: -kv[1])[:8])}"
                # span invariants per file, using the on-disk bytes
                checked = bad = 0
                examples = []
                for f in data.get("files", []):
                    try:
                        src = open(os.path.join(root, f["path"]), "rb").read()
                    except OSError:
                        continue
                    c, b, ex = span_problems(src, f, limit=2)
                    checked += c
                    bad += b
                    examples.extend(f"{f['path']}: {e}" for e in ex[:1])
                note += f" spans={checked} checked/{bad} bad"
                if examples:
                    note += f" e.g. {examples[:2]}"
            elif doc:
                note = f"lint summary={doc['data']['summary']}"
            else:
                note = f"err={first_line(result['err'], 200)!r}"
            say(f"{name:14s} dart_files={dart_files:5d} {command[0]:16s} exit={result['code']} {result['secs']:.1f}s rss={result['rss_mb']:.0f}MB out={len(result['out'])//1024}KB {note}")


def main():
    section = sys.argv[1]
    if not os.path.exists(BIN):
        say(f"binary not found: {BIN}")
        sys.exit(2)
    say(f"platform={sys.platform} bin={BIN}")
    {"battery": section_battery, "perf": section_perf, "projects": section_projects,
     "cli": section_cli, "fuzz": section_fuzz, "corpus": section_corpus}[section]()
    with open(os.path.join(OUT, f"runtime_{section}.txt"), "w", encoding="utf-8") as handle:
        handle.write("\n".join(LINES) + "\n")
    shutil.rmtree(TMP, ignore_errors=True)


if __name__ == "__main__":
    main()
