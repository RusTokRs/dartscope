from __future__ import annotations

import base64
import io
import tarfile
from pathlib import Path

PAYLOAD_PARTS = [
    Path('.github/payloads/flutter002.chunk01.b64'),
    Path('.github/payloads/flutter002.chunk02.b64'),
    Path('.github/payloads/flutter002.chunk03.b64'),
    Path('.github/payloads/flutter002.tail01.b64'),
    Path('.github/payloads/flutter002.tail02.b64'),
    Path('.github/payloads/flutter002.tail03.b64'),
    Path('.github/payloads/flutter002.tail04.b64'),
    Path('.github/payloads/flutter002.tail05.b64'),
    Path('.github/payloads/flutter002.tail06.b64'),
]
EXPECTED_SHA256 = '1a274d4242cbac5cae01fab1ba135bcd622979f5e5289774d034d984af80393f'


def main() -> None:
    import hashlib

    archive_bytes = base64.b64decode(''.join(path.read_text() for path in PAYLOAD_PARTS))
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

    obsolete = Path('crates/dartscope-parse/src/flutter_hints.rs')
    if obsolete.exists():
        obsolete.unlink()


if __name__ == '__main__':
    main()
