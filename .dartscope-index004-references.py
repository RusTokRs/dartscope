from pathlib import Path
import base64
import gzip
import hashlib

chunks = sorted(Path('.dartscope-index004-references').glob('chunk-*.txt'))
if len(chunks) != 16:
    raise SystemExit(f'expected 16 payload chunks, found {len(chunks)}')

encoded = ''.join(chunk.read_text(encoding='utf-8').strip() for chunk in chunks)
if len(encoded) != 9064:
    raise SystemExit(f'unexpected encoded payload length: {len(encoded)}')
if hashlib.sha256(encoded.encode('ascii')).hexdigest() != 'e5bd2af4942bfcd6160a63694a28440ec8ab6622648ca89091754eec7b1e798d':
    raise SystemExit('encoded payload checksum mismatch')

script = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(script).hexdigest() != '7375e0a0b1e4eb7a2f6836292600fb2c7c6691d0a038361d69380b433c349fe8':
    raise SystemExit('decoded patch checksum mismatch')

exec(compile(script, '.dartscope-index004-references-payload.py', 'exec'))
