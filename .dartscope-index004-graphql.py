from pathlib import Path
import base64
import gzip
import hashlib

parts = sorted(Path('.dartscope-index004-graphql').glob('part-*.txt'))
if len(parts) != 3:
    raise SystemExit(f'expected 3 payload parts, found {len(parts)}')

encoded = ''.join(part.read_text(encoding='utf-8').strip() for part in parts)
transport_typo = 'D ndzi'
if encoded.count(transport_typo) != 1:
    raise SystemExit('expected one known transport typo in payload part 00')
encoded = encoded.replace(transport_typo, 'D ndzi'.replace(' ', 'N'))

if len(encoded) != 8728:
    raise SystemExit(f'unexpected encoded payload length: {len(encoded)}')
if hashlib.sha256(encoded.encode('ascii')).hexdigest() != '5a1149c9000fba68400c7f1a167bed88fec65738947a9f4e4c8a9bb1f4d79d42':
    raise SystemExit('encoded payload checksum mismatch')

script = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(script).hexdigest() != '3892662e7c74aec9ff357f3bd3ca07a420e3fa9b5911ec14ed2919832428c211':
    raise SystemExit('decoded patch checksum mismatch')

exec(compile(script, '.dartscope-index004-graphql-payload.py', 'exec'))
