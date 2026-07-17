#!/usr/bin/env python3
"""Check repository facts that must stay synchronized across release and roadmap files."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_PACKAGES = 9
STALE_PLAN_FRAGMENTS = (
    "Baseline reviewed on 2026-07-16.",
    "Rust workspace and eight crates",
    "Lint/rule engine | planned | crate not created",
)


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main() -> None:
    failures: list[str] = []
    warnings: list[str] = []

    metadata = json.loads(
        run("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1")
    )
    packages = {package["name"] for package in metadata["packages"]}
    release_order = {
        line.strip()
        for line in (ROOT / "tools/release-crates.txt").read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }
    if len(packages) != EXPECTED_PACKAGES:
        failures.append(
            f"workspace contains {len(packages)} packages; expected {EXPECTED_PACKAGES}"
        )
    if packages != release_order:
        failures.append(
            f"workspace/release package sets differ: workspace={sorted(packages)}, "
            f"release={sorted(release_order)}"
        )

    plan = (ROOT / "docs/development/dartscope-library-plan.md").read_text(encoding="utf-8")
    for fragment in STALE_PLAN_FRAGMENTS:
        if fragment in plan:
            failures.append(f"roadmap contains stale text: {fragment!r}")

    release_workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    if "run: bash tools/publish-crates.sh" not in release_workflow:
        failures.append("release workflow must invoke publish-crates.sh through Bash")

    publish_mode = run("git", "ls-files", "--stage", "tools/publish-crates.sh").split()[0]
    if publish_mode != "100755":
        failures.append(f"tools/publish-crates.sh Git mode is {publish_mode}, expected 100755")

    if os.environ.get("DARTSCOPE_VERIFY_RELEASE_TAG") == "1":
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        version = str(manifest["workspace"]["package"]["version"])
        changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
        released_heading = f"## [{version}] - " in changelog
        tag = f"v{version}"
        tag_exists = subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}"],
            cwd=ROOT,
            check=False,
        ).returncode == 0
        if released_heading != tag_exists:
            failures.append(
                f"release-state mismatch for {version}: heading={released_heading}, tag={tag_exists}"
            )

    mutable_actions: list[str] = []
    ignored_workflows = {
        "audit-report.yml",
        "audit-roadmap-finalize.yml",
        "full-repository-audit.yml",
        "publish-mode-probe.yml",
        "repository-audit-observable.yml",
    }
    for workflow in sorted((ROOT / ".github/workflows").glob("*.yml")):
        if workflow.name in ignored_workflows:
            continue
        for line_number, line in enumerate(workflow.read_text(encoding="utf-8").splitlines(), 1):
            text = line.strip()
            normalized = text[2:].strip() if text.startswith("- ") else text
            if not normalized.startswith("uses:") or "@" not in normalized:
                continue
            reference = normalized.rsplit("@", 1)[1].split()[0]
            if len(reference) != 40 or any(
                character not in "0123456789abcdefABCDEF" for character in reference
            ):
                mutable_actions.append(f"{workflow.relative_to(ROOT)}:{line_number}: {text}")
    if mutable_actions:
        warnings.append(
            "mutable third-party Action references remain for DS-CI-003:\n  "
            + "\n  ".join(mutable_actions)
        )
        if os.environ.get("DARTSCOPE_REQUIRE_PINNED_ACTIONS") == "1":
            failures.append("permanent workflows contain mutable Action references")

    for warning in warnings:
        print(f"warning: {warning}")
    if failures:
        raise SystemExit("repository consistency failed:\n- " + "\n- ".join(failures))
    print("repository consistency passed")


if __name__ == "__main__":
    main()
