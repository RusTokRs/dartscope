#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CI = ROOT / ".github/workflows/ci.yml"
POLICY = ROOT / "tools/dependency-exceptions.toml"
CHECKER = ROOT / "tools/check-dependency-policy.py"
TESTS = ROOT / "tools/tests/test_dependency_policy.py"
AUDIT = ROOT / ".cargo/audit.toml"
DOC = ROOT / "docs/development/dependency-quality.md"
ROADMAP = ROOT / "docs/development/dartscope-library-plan.md"
CHANGELOG = ROOT / "CHANGELOG.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


POLICY.write_text('''# Every exception must be mirrored by the native tool configuration and include review metadata.
version = 1
cargo_audit_version = "0.22.2"
cargo_machete_version = "0.9.2"

# Example RustSec exception:
# [[rustsec_advisory]]
# id = "RUSTSEC-2099-0001"
# owner = "@maintainer"
# rationale = "The affected API and feature are not built or called by DartScope."
# expires_on = "2099-01-31"

# Example cargo-machete exception:
# [[unused_dependency]]
# manifest = "crates/example/Cargo.toml"
# dependency = "generated-runtime"
# owner = "@maintainer"
# rationale = "The dependency is referenced only from checked-in generated source."
# expires_on = "2099-01-31"
''', encoding="utf-8")

AUDIT.parent.mkdir(parents=True, exist_ok=True)
AUDIT.write_text('''[advisories]
ignore = []
informational_warnings = ["unmaintained"]
severity_threshold = "low"

[output]
deny = ["unmaintained"]
show_tree = true

[yanked]
enabled = true
update_index = true
''', encoding="utf-8")

CHECKER.write_text('''#!/usr/bin/env python3
"""Validate dependency-tool pins and reviewable exception metadata."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import date
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = Path("tools/dependency-exceptions.toml")
AUDIT_PATH = Path(".cargo/audit.toml")
CI_PATH = Path(".github/workflows/ci.yml")
ADVISORY_PATTERN = re.compile(r"^RUSTSEC-[0-9]{4}-[0-9]{4}$")
VERSION_PATTERN = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


@dataclass(frozen=True)
class PolicyFailure:
    location: str
    message: str

    def render(self) -> str:
        return f"{self.location}: {self.message}"


def read_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def parse_date(value: object, location: str, failures: list[PolicyFailure]) -> date | None:
    if isinstance(value, date):
        return value
    if isinstance(value, str):
        try:
            return date.fromisoformat(value)
        except ValueError:
            pass
    failures.append(PolicyFailure(location, "expires_on must be an ISO YYYY-MM-DD date"))
    return None


def validate_exception_fields(
    entry: dict,
    location: str,
    today: date,
    failures: list[PolicyFailure],
) -> None:
    owner = entry.get("owner")
    rationale = entry.get("rationale")
    if not isinstance(owner, str) or not owner.strip():
        failures.append(PolicyFailure(location, "owner must be a non-empty string"))
    if not isinstance(rationale, str) or len(rationale.strip()) < 20:
        failures.append(PolicyFailure(location, "rationale must contain at least 20 characters"))
    expires_on = parse_date(entry.get("expires_on"), location, failures)
    if expires_on is not None and expires_on < today:
        failures.append(
            PolicyFailure(location, f"exception expired on {expires_on.isoformat()}")
        )


def native_unused_ignores(root: Path, failures: list[PolicyFailure]) -> set[tuple[str, str]]:
    manifests = [root / "Cargo.toml", *sorted((root / "crates").glob("*/Cargo.toml"))]
    ignores: set[tuple[str, str]] = set()
    for manifest in manifests:
        data = read_toml(manifest)
        relative = manifest.relative_to(root).as_posix()
        package_metadata = data.get("package", {}).get("metadata", {}).get("cargo-machete", {})
        workspace_metadata = data.get("workspace", {}).get("metadata", {}).get("cargo-machete", {})
        for table_name, table in (("package", package_metadata), ("workspace", workspace_metadata)):
            values = table.get("ignored", []) if isinstance(table, dict) else []
            if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
                failures.append(
                    PolicyFailure(
                        f"{relative}:{table_name}.metadata.cargo-machete.ignored",
                        "ignored must be an array of dependency names",
                    )
                )
                continue
            for dependency in values:
                ignores.add((relative, dependency))
    return ignores


def check_policy(root: Path = ROOT, today: date | None = None) -> list[PolicyFailure]:
    today = today or date.today()
    failures: list[PolicyFailure] = []
    policy_path = root / POLICY_PATH
    audit_path = root / AUDIT_PATH
    ci_path = root / CI_PATH

    for path in (policy_path, audit_path, ci_path):
        if not path.is_file():
            failures.append(PolicyFailure(path.relative_to(root).as_posix(), "required file is missing"))
    if failures:
        return failures

    policy = read_toml(policy_path)
    if policy.get("version") != 1:
        failures.append(PolicyFailure(POLICY_PATH.as_posix(), "version must equal 1"))

    audit_version = policy.get("cargo_audit_version")
    machete_version = policy.get("cargo_machete_version")
    for key, value in (
        ("cargo_audit_version", audit_version),
        ("cargo_machete_version", machete_version),
    ):
        if not isinstance(value, str) or VERSION_PATTERN.fullmatch(value) is None:
            failures.append(PolicyFailure(POLICY_PATH.as_posix(), f"{key} must be x.y.z"))

    advisories = policy.get("rustsec_advisory", [])
    unused = policy.get("unused_dependency", [])
    if not isinstance(advisories, list):
        failures.append(PolicyFailure(POLICY_PATH.as_posix(), "rustsec_advisory must be an array of tables"))
        advisories = []
    if not isinstance(unused, list):
        failures.append(PolicyFailure(POLICY_PATH.as_posix(), "unused_dependency must be an array of tables"))
        unused = []

    advisory_ids: set[str] = set()
    for index, entry in enumerate(advisories):
        location = f"{POLICY_PATH.as_posix()}:rustsec_advisory[{index}]"
        if not isinstance(entry, dict):
            failures.append(PolicyFailure(location, "entry must be a table"))
            continue
        advisory_id = entry.get("id")
        if not isinstance(advisory_id, str) or ADVISORY_PATTERN.fullmatch(advisory_id) is None:
            failures.append(PolicyFailure(location, "id must match RUSTSEC-YYYY-NNNN"))
        elif advisory_id in advisory_ids:
            failures.append(PolicyFailure(location, f"duplicate advisory {advisory_id}"))
        else:
            advisory_ids.add(advisory_id)
        validate_exception_fields(entry, location, today, failures)

    unused_entries: set[tuple[str, str]] = set()
    for index, entry in enumerate(unused):
        location = f"{POLICY_PATH.as_posix()}:unused_dependency[{index}]"
        if not isinstance(entry, dict):
            failures.append(PolicyFailure(location, "entry must be a table"))
            continue
        manifest = entry.get("manifest")
        dependency = entry.get("dependency")
        if not isinstance(manifest, str) or not manifest.endswith("Cargo.toml"):
            failures.append(PolicyFailure(location, "manifest must name a Cargo.toml path"))
        if not isinstance(dependency, str) or not dependency.strip():
            failures.append(PolicyFailure(location, "dependency must be a non-empty string"))
        if isinstance(manifest, str) and isinstance(dependency, str):
            key = (Path(manifest).as_posix(), dependency)
            if key in unused_entries:
                failures.append(PolicyFailure(location, f"duplicate unused-dependency exception {key}"))
            unused_entries.add(key)
        validate_exception_fields(entry, location, today, failures)

    audit = read_toml(audit_path)
    native_advisories = audit.get("advisories", {}).get("ignore", [])
    if not isinstance(native_advisories, list) or not all(
        isinstance(value, str) for value in native_advisories
    ):
        failures.append(PolicyFailure(AUDIT_PATH.as_posix(), "advisories.ignore must be an array of IDs"))
        native_advisory_ids: set[str] = set()
    else:
        native_advisory_ids = set(native_advisories)
    if native_advisory_ids != advisory_ids:
        failures.append(
            PolicyFailure(
                AUDIT_PATH.as_posix(),
                f"native RustSec ignores {sorted(native_advisory_ids)} do not match policy {sorted(advisory_ids)}",
            )
        )

    native_unused = native_unused_ignores(root, failures)
    if native_unused != unused_entries:
        failures.append(
            PolicyFailure(
                POLICY_PATH.as_posix(),
                f"native cargo-machete ignores {sorted(native_unused)} do not match policy {sorted(unused_entries)}",
            )
        )

    ci_text = ci_path.read_text(encoding="utf-8")
    required_ci = {
        f"cargo-audit --version {audit_version}": "cargo-audit install pin is missing",
        f"cargo-machete --version {machete_version}": "cargo-machete install pin is missing",
        "python3 tools/check-dependency-policy.py": "dependency policy validation step is missing",
        "cargo audit": "RustSec audit command is missing",
        "cargo machete": "unused-dependency command is missing",
        "schedule:": "weekly dependency scan schedule is missing",
    }
    for fragment, message in required_ci.items():
        if fragment not in ci_text:
            failures.append(PolicyFailure(CI_PATH.as_posix(), message))

    return failures


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--today", type=date.fromisoformat)
    args = parser.parse_args()
    failures = check_policy(args.root.resolve(), args.today)
    if failures:
        for failure in failures:
            print(f"error: {failure.render()}", file=sys.stderr)
        raise SystemExit(1)
    print("dependency exception policy passed")


if __name__ == "__main__":
    main()
''', encoding="utf-8")

TESTS.write_text('''from __future__ import annotations

from datetime import date
import importlib.util
from pathlib import Path
import tempfile
import textwrap
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "check-dependency-policy.py"
SPEC = importlib.util.spec_from_file_location("dependency_policy", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DependencyPolicyTests(unittest.TestCase):
    def fixture(self, policy: str, audit_ignores: str = "", manifest_metadata: str = "") -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "tools").mkdir()
        (root / ".cargo").mkdir()
        (root / ".github/workflows").mkdir(parents=True)
        (root / "crates/example").mkdir(parents=True)
        (root / "tools/dependency-exceptions.toml").write_text(
            textwrap.dedent(policy), encoding="utf-8"
        )
        (root / ".cargo/audit.toml").write_text(
            f"[advisories]\nignore = [{audit_ignores}]\n", encoding="utf-8"
        )
        (root / "Cargo.toml").write_text(
            "[workspace]\nmembers = [\"crates/example\"]\n", encoding="utf-8"
        )
        (root / "crates/example/Cargo.toml").write_text(
            "[package]\nname = \"example\"\nversion = \"0.0.0\"\n" + manifest_metadata,
            encoding="utf-8",
        )
        (root / ".github/workflows/ci.yml").write_text(
            "schedule:\n  - cron: '17 6 * * 1'\n"
            "cargo install cargo-audit --version 0.22.2 --locked\n"
            "cargo install cargo-machete --version 0.9.2 --locked\n"
            "python3 tools/check-dependency-policy.py\n"
            "cargo audit\n"
            "cargo machete\n",
            encoding="utf-8",
        )
        return root

    def test_empty_policy_is_valid(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """
        )
        self.assertEqual(MODULE.check_policy(root, date(2026, 7, 18)), [])

    def test_exception_requires_owner_rationale_and_future_expiration(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            [[rustsec_advisory]]
            id = "RUSTSEC-2026-0001"
            owner = ""
            rationale = "short"
            expires_on = "2026-07-17"
            """,
            audit_ignores='"RUSTSEC-2026-0001"',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("owner" in message for message in messages))
        self.assertTrue(any("rationale" in message for message in messages))
        self.assertTrue(any("expired" in message for message in messages))

    def test_native_rustsec_ignore_must_match_policy(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """,
            audit_ignores='"RUSTSEC-2026-0001"',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("do not match policy" in message for message in messages))

    def test_native_machete_ignore_requires_matching_review_entry(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """,
            manifest_metadata='\n[package.metadata.cargo-machete]\nignored = ["serde"]\n',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("cargo-machete ignores" in message for message in messages))

    def test_reviewed_unused_exception_matches_native_metadata(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            [[unused_dependency]]
            manifest = "crates/example/Cargo.toml"
            dependency = "serde"
            owner = "@maintainer"
            rationale = "Used only by generated source checked in during release validation."
            expires_on = "2026-08-01"
            """,
            manifest_metadata='\n[package.metadata.cargo-machete]\nignored = ["serde"]\n',
        )
        self.assertEqual(MODULE.check_policy(root, date(2026, 7, 18)), [])


if __name__ == "__main__":
    unittest.main()
''', encoding="utf-8")

DOC.write_text('''---
id: doc://docs/development/dependency-quality.md
kind: development_contract
language: en
source_language: en
status: active
---

# Dependency Security And Hygiene

Permanent CI installs exact `cargo-audit 0.22.2` and `cargo-machete 0.9.2` releases with Cargo's
`--locked` installation mode. The dependency job runs on pushes, pull requests, manual dispatches, and a
weekly schedule. It is read-only and participates in the aggregate `dartscope/ci` result.

## Exception Policy

`tools/dependency-exceptions.toml` is the review source of truth. Every RustSec advisory or unused-
dependency exception must include:

- the exact advisory ID or manifest/dependency pair;
- a non-empty owner;
- a concrete rationale of at least 20 characters;
- an ISO expiration date that has not passed.

RustSec IDs must match `.cargo/audit.toml` exactly. Unused-dependency exceptions must match native
`package.metadata.cargo-machete.ignored` or `workspace.metadata.cargo-machete.ignored` entries exactly.
The checker rejects either an undocumented native ignore or a policy entry not applied to its tool.
Empty exception lists are the preferred baseline.

`cargo-audit` denies known vulnerabilities, yanked dependencies, and configured unmaintained warnings.
`cargo-machete` is intentionally run without `--with-metadata`: its static scan cannot mutate `Cargo.lock`
and any false positive must pass through the same expiring review policy rather than being silently
suppressed.

## Maintenance Boundary

Tool versions are duplicated deliberately in CI and the policy file; `check-dependency-policy.py` rejects
pin drift. Updating either tool requires reviewing its release, Rust 1.95 compatibility, output behavior,
and the complete exception list. Network or registry bootstrap failures are infrastructure failures and
must not be converted into dependency allowlist entries.
''', encoding="utf-8")

replace_once(
    CI,
    """  workflow_dispatch:

permissions:
""",
    """  workflow_dispatch:
  schedule:
    - cron: '17 6 * * 1'

permissions:
""",
)
replace_once(
    CI,
    """  quality:
    name: Quality gates
""",
    """  dependency_quality:
    name: Dependency security and hygiene
    needs: workflow_policy
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          persist-credentials: false
      - name: Install Rust 1.95
        run: rustup toolchain install 1.95.0 --profile minimal
      - name: Validate dependency exception policy
        run: python3 tools/check-dependency-policy.py
      - name: Install pinned dependency tools
        run: |
          cargo +1.95.0 install cargo-audit --version 0.22.2 --locked
          cargo +1.95.0 install cargo-machete --version 0.9.2 --locked
      - name: Run RustSec advisory audit
        run: cargo +1.95.0 audit
      - name: Detect unused dependencies
        run: cargo +1.95.0 machete

  quality:
    name: Quality gates
""",
)
replace_once(
    CI,
    """    needs: [workflow_policy, quality, test, edition_2024]
""",
    """    needs: [workflow_policy, dependency_quality, quality, test, edition_2024]
""",
)
replace_once(
    CI,
    """              '${{ needs.workflow_policy.result }}',
              '${{ needs.quality.result }}',
""",
    """              '${{ needs.workflow_policy.result }}',
              '${{ needs.dependency_quality.result }}',
              '${{ needs.quality.result }}',
""",
)

replace_once(
    ROADMAP,
    """### DS-QUALITY-001: Durable Security, Fuzzing, And Performance Gates

Status: ready. Priority: P1. Prerequisite: DS-AUDIT-001.

Required work:
""",
    """### DS-QUALITY-001: Durable Security, Fuzzing, And Performance Gates

Status: in progress. Priority: P1. Prerequisite: DS-AUDIT-001.

Progress (2026-07-18):

1. Added exact `cargo-audit 0.22.2` and `cargo-machete 0.9.2` installs to permanent read-only CI,
   including weekly scheduled execution and aggregate-status participation.
2. Added a versioned exception registry and checker that requires owner, rationale, and unexpired review
   date, and rejects drift from native RustSec or cargo-machete ignore configuration.
3. Added focused policy tests for missing metadata, expired exceptions, native-ignore drift, and a valid
   reviewed unused-dependency exception. The initial exception registry is empty.

Findings and limits:

- `cargo-machete` is a fast static detector and can produce false positives. It runs without
  `--with-metadata` so CI cannot modify `Cargo.lock`; any ignore must be explicit and expiring.
- Dependency-tool installation and RustSec database refresh remain network-dependent CI bootstrap steps.
  Infrastructure failures must not be converted into advisory or unused-dependency exceptions.
- Exact tool pins are duplicated in CI and policy intentionally; policy validation rejects version drift.

Required work:
""",
)

replace_once(
    CHANGELOG,
    """- Deterministic retained-cache payload metrics and informational 1k/10k index/lint update-time baselines
  without flaky absolute timing thresholds.
""",
    """- Deterministic retained-cache payload metrics and informational 1k/10k index/lint update-time baselines
  without flaky absolute timing thresholds.
- Pinned RustSec advisory and unused-dependency CI gates with expiring, owner-attributed exception policy.
""",
)

print("DS-QUALITY-001 dependency gate slice applied")
