#!/usr/bin/env python3
"""Apply the reviewed DS-QUALITY-001 bounded-fuzzing payload."""

from __future__ import annotations

import base64
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BASE_SHA = "d1af6c52477c67a86716775cacf8269fc86b565c"
PAYLOAD_SHA256 = "75216162e27c99ee41648167ea91a511032f42881bfb098a86de06184efa60a6"
ANCHORS = {
    ".github/workflows/ci.yml": "2e551573b1939d50aa3cd99155e10f63a91613d4d6662f5791b50250dca999f5",
    "CHANGELOG.md": "a6779327cf9a53ff77c82830115ae24c1662f4caf5c505fcc2be8df5a22fd47a",
    "crates/dartscope-parse/Cargo.toml": "bb14ad4eba6a63d4b38602219b9ff703f51e94393d8e704cc3235ae2af1d9702",
    "crates/dartscope-parse/src/lib.rs": "01ee1d7157c62ed4514d6ef6b91d261facba8bc73599ab8ce194bf49da3adf4c",
    "docs/development/dartscope-library-plan.md": "82c47087c2abaf653f5e025fbb1c3b4367f0f71fbf83bc1681db2a5a0525c04c",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", BASE_SHA, "HEAD"],
        cwd=ROOT,
        check=True,
    )
    for relative, expected in ANCHORS.items():
        actual = digest(ROOT / relative)
        if actual != expected:
            raise SystemExit(f"anchor drift for {relative}: {actual}")

    chunks = sorted((ROOT / "tools").glob(".ds_quality001_fuzz_payload_*"))
    if [path.name.rsplit("_", 1)[-1] for path in chunks] != [f"{index:02d}" for index in range(7)]:
        raise SystemExit("expected exactly seven ordered fuzz payload chunks")
    encoded = "".join(path.read_text(encoding="ascii") for path in chunks)
    payload = base64.b64decode(encoded, validate=True)
    if hashlib.sha256(payload).hexdigest() != PAYLOAD_SHA256:
        raise SystemExit("fuzz payload digest mismatch")

    with tempfile.NamedTemporaryFile(suffix=".tar.gz") as handle:
        handle.write(payload)
        handle.flush()
        with tarfile.open(handle.name, "r:gz") as archive:
            for member in archive.getmembers():
                path = Path(member.name)
                if path.is_absolute() or ".." in path.parts or not member.isfile():
                    raise SystemExit(f"unsafe payload member: {member.name}")
                target = (ROOT / path).resolve()
                if ROOT.resolve() not in target.parents:
                    raise SystemExit(f"payload escapes repository: {member.name}")
                target.parent.mkdir(parents=True, exist_ok=True)
                source = archive.extractfile(member)
                if source is None:
                    raise SystemExit(f"missing payload bytes: {member.name}")
                target.write_bytes(source.read())

    print("DS-QUALITY-001 bounded fuzzing payload applied")


if __name__ == "__main__":
    main()
