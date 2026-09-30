#!/usr/bin/env python3
"""Wire-level LSP probe for dartscope-lsp (engineering audit only).

Usage: lsp_probe.py PATH_TO_dartscope-lsp
"""
import json
import os
import queue
import subprocess
import sys
import threading
import time

EXE = sys.argv[1]
OUT = os.environ.get("PROBE_OUT", "/tmp")
LINES = []


def say(text=""):
    LINES.append(text)
    print(text, flush=True)


class Lsp:
    def __init__(self):
        self.proc = subprocess.Popen([EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.inbox = queue.Queue()
        self.next_id = 1
        self.stderr_chunks = []
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _read_stderr(self):
        for chunk in iter(lambda: self.proc.stderr.read(512), b""):
            self.stderr_chunks.append(chunk)

    def _read_stdout(self):
        stream = self.proc.stdout
        while True:
            length = None
            while True:
                line = stream.readline()
                if not line:
                    self.inbox.put(None)
                    return
                line = line.strip()
                if not line:
                    break
                if line.lower().startswith(b"content-length:"):
                    length = int(line.split(b":", 1)[1])
            if length is None:
                continue
            body = stream.read(length)
            try:
                self.inbox.put(json.loads(body))
            except ValueError:
                self.inbox.put({"_unparseable": body[:200].decode("utf-8", "replace")})

    def raw(self, data):
        try:
            self.proc.stdin.write(data)
            self.proc.stdin.flush()
        except (BrokenPipeError, OSError):
            pass

    def send(self, payload):
        body = json.dumps(payload).encode("utf-8")
        self.raw(b"Content-Length: %d\r\n\r\n" % len(body) + body)

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def request(self, method, params, timeout=6):
        request_id = self.next_id
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        deadline = time.time() + timeout
        stash = []
        while time.time() < deadline:
            try:
                message = self.inbox.get(timeout=max(0.05, deadline - time.time()))
            except queue.Empty:
                break
            if message is None:
                return {"_closed": True, "_notifications": stash}
            if message.get("id") == request_id:
                message["_notifications"] = [m.get("method") for m in stash]
                return message
            stash.append(message)
        return {"_timeout": True, "_notifications": [m.get("method") for m in stash]}

    def drain(self, seconds=0.6):
        seen = []
        deadline = time.time() + seconds
        while time.time() < deadline:
            try:
                message = self.inbox.get(timeout=0.1)
            except queue.Empty:
                continue
            if message is None:
                seen.append({"_closed": True})
                break
            seen.append(message)
        return seen

    def alive(self):
        return self.proc.poll() is None

    def stderr_text(self):
        return b"".join(self.stderr_chunks).decode("utf-8", "replace").strip().splitlines()[:3]

    def close(self):
        try:
            self.proc.kill()
        except OSError:
            pass


def short(value, limit=300):
    text = json.dumps(value, ensure_ascii=False)
    return text if len(text) <= limit else text[:limit] + "…"


def start(root="file:///tmp/proj"):
    server = Lsp()
    reply = server.request(
        "initialize",
        {"processId": os.getpid(), "rootUri": root, "rootPath": "/tmp/proj",
         "capabilities": {"textDocument": {"definition": {"linkSupport": True}}},
         "workspaceFolders": [{"uri": root, "name": "proj"}], "clientInfo": {"name": "audit"}},
    )
    server.notify("initialized", {})
    return server, reply


def open_doc(server, uri, text, version=1):
    server.notify("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "dart", "version": version, "text": text}})


def pos_of(text, needle, nth=0, delta=0):
    index = -1
    for _ in range(nth + 1):
        index = text.index(needle, index + 1)
    index += delta
    prefix = text[:index]
    line = prefix.count("\n")
    col_text = prefix[prefix.rfind("\n") + 1:]
    utf16 = len(col_text.encode("utf-16-le")) // 2
    return {"line": line, "character": utf16}


def main():
    say(f"## G. dartscope-lsp wire probe ({EXE})")
    # G01 initialize response shape
    server, reply = start()
    result = reply.get("result", {})
    caps = result.get("capabilities", {})
    say(f"[G01] initialize -> top-level keys={sorted(result) if isinstance(result, dict) else result}")
    say(f"      capabilities keys={sorted(caps)}  (LSP requires camelCase: textDocumentSync, definitionProvider, referencesProvider, hoverProvider, documentSymbolProvider)")
    say(f"      raw={short(reply, 420)}")
    # G02 rootUri honoured?
    say("[G02] rootUri 'file:///tmp/proj' is sent as `rootUri`; the server's InitializeParams field is named `root_uri` (no camelCase rename) -> root ignored")
    # G03 simple same-file definition/hover/references/symbols
    uri = "file:///tmp/proj/lib/main.dart"
    text = "class Foo {\n  int x = 1;\n  void bar() {}\n}\nvoid main() {\n  Foo();\n  final a = 1;\n  print(a);\n}\n"
    open_doc(server, uri, text)
    diag_notifications = [m.get("method") for m in server.drain(0.8)]
    say(f"[G03] after didOpen, server notifications={diag_notifications} (expect textDocument/publishDiagnostics for clients to show diagnostics)")
    cases = [
        ("definition on `Foo()` call", "textDocument/definition", {"textDocument": {"uri": uri}, "position": pos_of(text, "Foo();", 0, 1)}),
        ("definition on local `a` read", "textDocument/definition", {"textDocument": {"uri": uri}, "position": pos_of(text, "print(a)", 0, 6)}),
        ("hover on `Foo()` call", "textDocument/hover", {"textDocument": {"uri": uri}, "position": pos_of(text, "Foo();", 0, 1)}),
        ("references on `Foo()` call", "textDocument/references", {"textDocument": {"uri": uri}, "position": pos_of(text, "Foo();", 0, 1), "context": {"includeDeclaration": True}}),
        ("documentSymbol", "textDocument/documentSymbol", {"textDocument": {"uri": uri}}),
    ]
    for label, method, params in cases:
        answer = server.request(method, params)
        say(f"[G04] {label}: {short({k: v for k, v in answer.items() if k != '_notifications'}, 500)}")
    # G05 positions beyond line length (LSP: clamp to line end) / position in a line past EOF
    answer = server.request("textDocument/definition", {"textDocument": {"uri": uri}, "position": {"line": 5, "character": 500}})
    say(f"[G05] definition with character beyond line length (spec: clamp): {short({k: v for k, v in answer.items() if k != '_notifications'}, 300)}")
    # G06 malformed params -> must get an error response
    answer = server.request("textDocument/definition", {"oops": True}, timeout=3)
    say(f"[G06] malformed params on a request -> {'NO RESPONSE (client would hang)' if answer.get('_timeout') else short(answer, 240)}")
    # G07 unknown request method
    answer = server.request("workspace/symbol", {"query": "x"}, timeout=3)
    say(f"[G07] unsupported request `workspace/symbol` -> {short({k: v for k, v in answer.items() if k != '_notifications'}, 240)} (spec: error -32601 MethodNotFound)")
    server.close()

    # G08 reversed/out-of-range incremental edits must not crash the server
    for label, change in (
        ("reversed range (start > end)", {"range": {"start": {"line": 0, "character": 6}, "end": {"line": 0, "character": 2}}, "text": "X"}),
        ("end beyond line length", {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 999}}, "text": "class Q {}"}),
        ("end beyond last line", {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 50, "character": 0}}, "text": "class Q {}"}),
    ):
        server, _ = start()
        open_doc(server, uri, "class Foo {}\nclass Bar {}\n")
        server.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": 2}, "contentChanges": [change]})
        time.sleep(0.4)
        alive = server.alive()
        answer = server.request("textDocument/documentSymbol", {"textDocument": {"uri": uri}}, timeout=3) if alive else None
        names = None
        if answer and isinstance(answer.get("result"), list):
            names = [s.get("name") for s in answer["result"]]
        say(f"[G08] didChange {label}: server alive={alive} exit={server.proc.poll()} symbols_after={names} stderr={server.stderr_text()}")
        server.close()

    # G09 cross-file go-to-definition with percent-encoded / non-ASCII URIs
    for label, base in (("ascii", "file:///tmp/proj/lib"),
                        ("space", "file:///tmp/my%20proj/lib"),
                        ("cyrillic", "file:///tmp/%D0%BF%D1%80%D0%BE%D0%B5%D0%BA%D1%82/lib"),
                        ("windows drive (VS Code style)", "file:///c%3A/Users/dev/proj/lib")):
        server, _ = start()
        a_uri, b_uri = f"{base}/a.dart", f"{base}/b.dart"
        a_text = "import 'b.dart';\nvoid main() { Target(); }\n"
        b_text = "// комментарий\nclass Target {}\n"
        open_doc(server, b_uri, b_text)
        open_doc(server, a_uri, a_text)
        server.drain(0.3)
        answer = server.request("textDocument/definition", {"textDocument": {"uri": a_uri}, "position": pos_of(a_text, "Target();", 0, 1)}, timeout=4)
        locations = answer.get("result")
        first = locations[0] if isinstance(locations, list) and locations else None
        ok = bool(first) and first.get("uri") == b_uri
        say(f"[G09] cross-file definition [{label}]: target uri returned={first.get('uri') if first else None!r} matches requested={ok} range={first.get('range') if first else None}")
        server.close()

    # G10 protocol robustness
    server, _ = start()
    server.raw(b"Content-Length: 99999999999\r\n\r\n{}")
    time.sleep(1.0)
    say(f"[G10] Content-Length: 99999999999 -> alive={server.alive()} exit={server.proc.poll()} stderr={server.stderr_text()}")
    server.close()
    server = Lsp()
    server.raw(b"Content-Length: 5\r\n\r\n{bad}")
    answer = server.request("initialize", {"capabilities": {}}, timeout=3)
    say(f"[G11] invalid JSON followed by a valid request: {'recovered' if answer.get('result') is not None else short(answer, 160)}; no error response sent for the bad message")
    server.close()
    server = Lsp()
    answer = server.request("textDocument/definition", {"textDocument": {"uri": uri}, "position": {"line": 0, "character": 0}}, timeout=3)
    say(f"[G12] request before `initialize`: {short({k: v for k, v in answer.items() if k != '_notifications'}, 240)} (spec: error -32002 ServerNotInitialized)")
    server.close()
    server, _ = start()
    server.request("shutdown", None, timeout=3)
    server.notify("exit", None)
    time.sleep(0.5)
    say(f"[G13] shutdown+exit -> exit code={server.proc.poll()} (spec: 0)")
    server.close()
    server, _ = start()
    server.notify("exit", None)
    time.sleep(0.5)
    say(f"[G14] exit WITHOUT shutdown -> exit code={server.proc.poll()} (spec: 1)")
    server.close()

    # G15 CRLF + non-BMP coordinates through the wire
    server, _ = start()
    text = "class A {}\r\n// 😀 emoji\r\nclass B {}\r\nvoid f() { B(); }\r\n"
    open_doc(server, uri, text)
    server.drain(0.3)
    target = pos_of(text, "B();", 0, 0)
    answer = server.request("textDocument/definition", {"textDocument": {"uri": uri}, "position": target}, timeout=4)
    say(f"[G15] CRLF + emoji: query at {target} -> {short(answer.get('result'), 300)} (expected range on line 2 of `class B`)")
    server.close()

    # G16 latency: rebuild-everything-per-edit and per-request context rebuild
    server, _ = start()
    docs = 150
    for index in range(docs):
        open_doc(server, f"file:///tmp/proj/lib/f{index}.dart",
                 f"import 'f{(index + 1) % docs}.dart';\nclass F{index} {{\n  void run() {{ F{(index + 1) % docs}().run(); }}\n}}\n")
    server.drain(0.2)
    started = time.time()
    answer = server.request("textDocument/documentSymbol", {"textDocument": {"uri": "file:///tmp/proj/lib/f0.dart"}}, timeout=120)
    opened = time.time() - started
    started = time.time()
    for edit in range(10):
        server.notify("textDocument/didChange", {"textDocument": {"uri": "file:///tmp/proj/lib/f0.dart", "version": edit + 2},
                                                 "contentChanges": [{"text": f"class F0 {{ int v{edit}; }}\n"}]})
    answer = server.request("textDocument/documentSymbol", {"textDocument": {"uri": "file:///tmp/proj/lib/f0.dart"}}, timeout=180)
    edits = time.time() - started
    started = time.time()
    definition = server.request("textDocument/definition", {"textDocument": {"uri": "file:///tmp/proj/lib/f1.dart"}, "position": {"line": 2, "character": 18}}, timeout=120)
    one_definition = time.time() - started
    say(f"[G16] {docs} open docs: queue drained after opens in {opened:.2f}s; 10 full-text edits + 1 request took {edits:.2f}s ({edits/10*1000:.0f} ms/edit); one definition request {one_definition*1000:.0f} ms")
    server.close()


main()
with open(os.path.join(OUT, "lsp_probe.txt"), "w", encoding="utf-8") as handle:
    handle.write("\n".join(LINES) + "\n")
