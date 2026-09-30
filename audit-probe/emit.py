#!/usr/bin/env python3
"""Emit a text file as chunked GitHub Actions notice annotations (audit probe only)."""
import argparse
import os
import re
import sys
import tempfile


def esc_data(text: str) -> str:
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def esc_prop(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _./\[\]=-]", "_", text)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("title")
    ap.add_argument("file")
    ap.add_argument("--chunk", type=int, default=20000)
    ap.add_argument("--max", type=int, default=6, dest="max_chunks")
    ap.add_argument("--tail", action="store_true", help="keep the tail instead of the head")
    ap.add_argument("--level", default="cycle")
    args = ap.parse_args()
    try:
        data = open(args.file, encoding="utf-8", errors="replace").read()
    except FileNotFoundError:
        data = "<missing file>"
    if not data:
        data = "<empty>"
    limit = args.chunk * args.max_chunks
    truncated = ""
    if len(data) > limit:
        if args.tail:
            data = data[-limit:]
            truncated = f"[TRUNCATED: head dropped, kept last {limit} chars]\n"
        else:
            data = data[:limit]
            truncated = f"[TRUNCATED: kept first {limit} chars]\n"
    data = truncated + data
    chunks = [data[i : i + args.chunk] for i in range(0, len(data), args.chunk)]
    total = len(chunks)
    counter_path = os.environ.get("ANNOT_COUNTER", os.path.join(tempfile.gettempdir(), "ds_annot_counter"))
    for index, chunk in enumerate(chunks, 1):
        level = args.level
        if level == "cycle":
            try:
                count = int(open(counter_path).read().strip() or 0)
            except (OSError, ValueError):
                count = 0
            level = ("notice", "warning", "error")[count % 3]
            with open(counter_path, "w") as handle:
                handle.write(str(count + 1))
        title = esc_prop(f"{args.title} [{index}/{total}]")
        print(f"::{level} title={title}::{esc_data(chunk)}")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
