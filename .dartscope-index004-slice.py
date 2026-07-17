from pathlib import Path
import base64
import gzip

parts = sorted(Path(".dartscope-index004-payload").glob("*.part"))
payload = "".join(part.read_text(encoding="utf-8").strip() for part in parts)
exec(gzip.decompress(base64.b64decode(payload)))
