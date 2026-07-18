#!/usr/bin/env python3
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
        "cargo +1.95.0 audit": "RustSec audit command is missing",
        "cargo +1.95.0 machete": "unused-dependency command is missing",
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
