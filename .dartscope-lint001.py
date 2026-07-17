from pathlib import Path
import base64
import gzip
import hashlib

parts = sorted(Path(".dartscope-lint001").glob("part-*.txt"))
if len(parts) != 6:
    raise SystemExit(f"expected 6 payload parts, found {len(parts)}")

encoded = "".join(part.read_text(encoding="utf-8").strip() for part in parts)
if len(encoded) != 14000:
    raise SystemExit(f"unexpected encoded payload length: {len(encoded)}")
if hashlib.sha256(encoded.encode("ascii")).hexdigest() != "2f993dedb36f21262973dba5f61187fb41533f8364cd18d528051ffe930965e3":
    raise SystemExit("encoded payload checksum mismatch")

script = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(script).hexdigest() != "dae4137c6adfe2eb0bcd84e4a89ccb1c6d29a566e12b9780017d932f7fb8af2f":
    raise SystemExit("decoded payload checksum mismatch")

exec(compile(script, ".dartscope-lint001-payload.py", "exec"))
