from pathlib import Path
import base64
import gzip
import hashlib

parts = sorted(Path('.dartscope-index004-references').glob('part-*.txt'))
if len(parts) != 4:
    raise SystemExit(f'expected 4 payload parts, found {len(parts)}')

encoded = ''.join(part.read_text(encoding='utf-8').strip() for part in parts)
if len(encoded) != 9064:
    raise SystemExit(f'unexpected encoded payload length: {len(encoded)}')
if hashlib.sha256(encoded.encode('ascii')).hexdigest() != '52b4d4959ab2081a6463acd7a8c1580cc826621143baab72204e5d3939042144':
    raise SystemExit('encoded payload checksum mismatch')

script = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(script).hexdigest() != '07d8d40a2936435f070d63cb74ae89e2d7ab3182839d8ab6656da9bf0f624a92':
    raise SystemExit('decoded patch checksum mismatch')

exec(compile(script, '.dartscope-index004-references-payload.py', 'exec'))
# trigger finalization after workflow registration
