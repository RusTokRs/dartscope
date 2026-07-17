from __future__ import annotations

from pathlib import Path
import hashlib
import shutil

ROOT = Path(__file__).resolve().parents[1]
STAGED = ROOT / ".dartscope-release001" / "files"
MANIFEST = ROOT / ".dartscope-release001" / "manifest.tsv"


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")


def copy_staged_files() -> None:
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        staged_path, target_path, expected_sha = line.split("\t")
        source = STAGED / staged_path
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if digest != expected_sha:
            raise SystemExit(f"{staged_path}: staged checksum mismatch")
        destination = ROOT / target_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


replace_once(
    "Cargo.toml",
    """[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT"
repository = "https://github.com/RusTokRs/dartscope"
description = "Rust toolkit for Dart and Flutter code intelligence"
""",
    """[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT"
repository = "https://github.com/RusTokRs/dartscope"
homepage = "https://github.com/RusTokRs/dartscope"
readme = "README.md"
keywords = ["dart", "flutter", "static-analysis", "code-intelligence"]
categories = ["development-tools"]
description = "Rust toolkit for Dart and Flutter code intelligence"
""",
)
replace_once(
    "Cargo.toml",
    """[workspace.dependencies]
dartscope-core = { path = "crates/dartscope-core" }
dartscope-parse = { path = "crates/dartscope-parse" }
dartscope-index = { path = "crates/dartscope-index" }
dartscope-lints = { path = "crates/dartscope-lints" }
dartscope-resolve = { path = "crates/dartscope-resolve" }
dartscope-flutter = { path = "crates/dartscope-flutter" }
dartscope-json = { path = "crates/dartscope-json" }
""",
    """[workspace.dependencies]
dartscope = { version = "0.1.0", path = "crates/dartscope" }
dartscope-core = { version = "0.1.0", path = "crates/dartscope-core" }
dartscope-parse = { version = "0.1.0", path = "crates/dartscope-parse" }
dartscope-index = { version = "0.1.0", path = "crates/dartscope-index" }
dartscope-lints = { version = "0.1.0", path = "crates/dartscope-lints" }
dartscope-resolve = { version = "0.1.0", path = "crates/dartscope-resolve" }
dartscope-flutter = { version = "0.1.0", path = "crates/dartscope-flutter" }
dartscope-json = { version = "0.1.0", path = "crates/dartscope-json" }
""",
)

package_metadata = {
    "crates/dartscope/Cargo.toml": (
        'description = "Thin umbrella crate for DartScope"\n',
        'https://docs.rs/dartscope',
    ),
    "crates/dartscope-core/Cargo.toml": (
        'description = "Core DartScope analysis model, spans, diagnostics, and ports"\n',
        'https://docs.rs/dartscope-core',
    ),
    "crates/dartscope-parse/Cargo.toml": (
        'description = "Conservative Dart and Flutter file analysis for DartScope"\n',
        'https://docs.rs/dartscope-parse',
    ),
    "crates/dartscope-index/Cargo.toml": (
        'description = "Project indexing and cross-file analysis for DartScope"\n',
        'https://docs.rs/dartscope-index',
    ),
    "crates/dartscope-lints/Cargo.toml": (
        'description = "Optional deterministic lint rules for normalized DartScope analysis"\n',
        'https://docs.rs/dartscope-lints',
    ),
    "crates/dartscope-resolve/Cargo.toml": (
        'description = "Package and URI resolution inputs for DartScope"\n',
        'https://docs.rs/dartscope-resolve',
    ),
    "crates/dartscope-flutter/Cargo.toml": (
        'description = "Flutter convention analysis for DartScope"\n',
        'https://docs.rs/dartscope-flutter',
    ),
    "crates/dartscope-json/Cargo.toml": (
        'description = "Versioned JSON contracts and serialization helpers for DartScope output"\n',
        'https://docs.rs/dartscope-json',
    ),
    "crates/dartscope-cli/Cargo.toml": (
        'description = "Small CLI wrapper for DartScope local workflows"\n',
        'https://docs.rs/dartscope-cli',
    ),
}
for path, (description, documentation) in package_metadata.items():
    replace_once(
        path,
        f"repository.workspace = true\n{description}",
        (
            "repository.workspace = true\n"
            "homepage.workspace = true\n"
            "readme.workspace = true\n"
            "keywords.workspace = true\n"
            "categories.workspace = true\n"
            f'documentation = "{documentation}"\n'
            f"{description}"
        ),
    )

replace_once(
    "crates/dartscope-cli/Cargo.toml",
    'dartscope = { path = "../dartscope", features = ["parse", "index", "json", "flutter"] }\n',
    'dartscope = { workspace = true, features = ["parse", "index", "json", "flutter"] }\n',
)

replace_once(
    "README.md",
    """A dedicated CI matrix verifies resolver 3 and edition 2024 on Linux and Windows for the
complete workspace, the umbrella crate without default features, and the umbrella crate
with all features. See
[`docs/development/rust-2024-edition.md`](docs/development/rust-2024-edition.md).

## Quick Start
""",
    """A dedicated CI matrix verifies resolver 3 and edition 2024 on Linux and Windows for the
complete workspace, the umbrella crate without default features, and the umbrella crate
with all features. See
[`docs/development/rust-2024-edition.md`](docs/development/rust-2024-edition.md).

## Release And Support

All nine crates carry crates.io-ready metadata and versioned internal dependencies. The release gate
builds every `.crate` archive in dependency order without publishing it. See the
[0.1 support matrix](docs/support-matrix.md), [release process](docs/release-process.md),
[changelog](CHANGELOG.md), and [security policy](SECURITY.md).

```powershell
python tools/check-release-packages.py
```

Publishing remains a manually dispatched, protected-environment action from an exact `v<version>`
tag; normal pushes and tags never publish crates.

## Quick Start
""",
)
replace_once(
    "README.md",
    """- [Reference strategy](docs/reference-strategy.md)
- [Library development plan](docs/development/dartscope-library-plan.md)
- [Rust code standards](docs/development/rust-code-standards.md)
- [Agent workflow](AGENTS.md)
- [Contributing](CONTRIBUTING.md)
""",
    """- [Reference strategy](docs/reference-strategy.md)
- [Library development plan](docs/development/dartscope-library-plan.md)
- [Rust code standards](docs/development/rust-code-standards.md)
- [Support matrix](docs/support-matrix.md)
- [Release process](docs/release-process.md)
- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)
- [Agent workflow](AGENTS.md)
- [Contributing](CONTRIBUTING.md)
""",
)

replace_once(
    "docs/development/dartscope-library-plan.md",
    """### DS-RELEASE-001: Publishable 0.1 Release

Status: planned. Priority: P3. Prerequisites: DS-JSON-001, DS-CLI-002.

Add complete package metadata, rustdoc coverage, changelog, security policy, crate
publish order, `cargo package` checks, release CI, and an explicit support matrix for
Rust, Dart, Flutter, and ecosystem conventions.
""",
    """### DS-RELEASE-001: Publishable 0.1 Release

Status: verified. Priority: P3. Prerequisites: DS-JSON-001, DS-CLI-002.

Implemented (2026-07-17):

1. Added inherited homepage, readme, keywords, categories, per-crate docs.rs links, and crates.io
   version requirements for every internal normal and development dependency.
2. Added a changelog and a private-reporting-first security policy for the `0.1` release line.
3. Added an executable nine-crate publish order with metadata/topology validation and generated
   `.crate` archive inspection.
4. Added release CI on exact Rust 1.95.0 with workspace, all-feature, rustdoc, package, and artifact
   gates.
5. Added a manually dispatched, tag-checked, protected-environment crates.io publishing path that
   waits for each dependency version before publishing consumers.
6. Added an explicit support matrix for Rust, host CI, Dart capabilities, Flutter conventions,
   ecosystem package ranges, and command-facing JSON contracts.

Verification:

- every package archive contains a normalized manifest and inherited README with no packaged path
  dependency;
- release metadata and publish-order topology are checked from `cargo metadata --locked`;
- exact Rust 1.95 formatting, Clippy, rustdoc, workspace tests, umbrella all-features tests, and all
  nine `cargo package --no-verify` archives pass before finalization.

See `CHANGELOG.md`, `SECURITY.md`, `docs/support-matrix.md`, and
`docs/release-process.md`.
""",
)

copy_staged_files()
