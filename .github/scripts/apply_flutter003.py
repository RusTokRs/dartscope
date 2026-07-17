from __future__ import annotations

import base64
import hashlib
import io
import tarfile
from pathlib import Path

PARTS = [
    Path('.github/payloads/flutter003.part01.b64'),
    Path('.github/payloads/flutter003.part02.b64'),
    Path('.github/payloads/flutter003.part03.b64'),
    Path('.github/payloads/flutter003.part04.b64'),
    Path('.github/payloads/flutter003.part05.b64'),
    Path('.github/payloads/flutter003.part06.b64'),
    Path('.github/payloads/flutter003.part07.b64'),
    Path('.github/payloads/flutter003.part08.b64'),
    Path('.github/payloads/flutter003.part09.b64'),
]
EXPECTED_SHA256 = 'ee64fe2a2d717a0e2aa9084dbd644900c509b883d7386cc9a20c404172a31228'


def main() -> None:
    encoded = ''.join(Path(path).read_text() for path in PARTS)
    archive_bytes = base64.b64decode(encoded)
    actual = hashlib.sha256(archive_bytes).hexdigest()
    if actual != EXPECTED_SHA256:
        raise RuntimeError(f'payload checksum mismatch: {actual}')

    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode='r:gz') as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            relative = Path(member.name)
            if relative.is_absolute() or '..' in relative.parts:
                raise RuntimeError(f'unsafe payload path: {member.name}')
            source = archive.extractfile(member)
            if source is None:
                raise RuntimeError(f'missing payload content: {member.name}')
            destination = Path(*[part for part in relative.parts if part != '.'])
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(source.read())


if __name__ == '__main__':
    main()
