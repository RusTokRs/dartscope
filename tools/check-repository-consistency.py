#!/usr/bin/env python3
"""Check repository facts that must stay synchronized across release and roadmap files."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_PACKAGES = 9
EXPECTED_WORKFLOWS = {"ci.yml", "release.yml"}
STALE_PLAN_FRAGMENTS = (
    "Baseline reviewed on 2026-07-16.",
    "Rust workspace and eight crates",
    "Lint/rule engine | planned | crate not created",
    "### DS-CI-003: Immutable Actions And CI Supply Chain\n\nStatus: ready.",
)


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main() -> None:
    failures: list[str] = []

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

    workflows = {
        path.name
        for path in (ROOT / ".github/workflows").iterdir()
        if path.suffix in {".yml", ".yaml"}
    }
    if workflows != EXPECTED_WORKFLOWS:
        failures.append(
            f"permanent workflow inventory changed: actual={sorted(workflows)}, "
            f"expected={sorted(EXPECTED_WORKFLOWS)}"
        )

    release_workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    if "run: bash tools/publish-crates.sh" not in release_workflow:
        failures.append("release workflow must invoke publish-crates.sh through Bash")
    for workflow_name in EXPECTED_WORKFLOWS:
        workflow = (ROOT / ".github/workflows" / workflow_name).read_text(encoding="utf-8")
        if "python3 tools/check-workflow-policy.py" not in workflow:
            failures.append(f"{workflow_name} must enforce the workflow policy")
        if "github.com/rhysd/actionlint/cmd/actionlint@v1.7.12" not in workflow:
            failures.append(f"{workflow_name} must run pinned actionlint v1.7.12")

    policy = subprocess.run(
        [sys.executable, "tools/check-workflow-policy.py"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if policy.returncode != 0:
        failures.append(
            "workflow policy failed:\n"
            + (policy.stdout + policy.stderr).strip()
        )

    tracked_python_artifacts = [
        path
        for path in run("git", "ls-files").splitlines()
        if "__pycache__/" in path or path.endswith((".pyc", ".pyo", ".pyd"))
    ]
    if tracked_python_artifacts:
        failures.append(
            "generated Python artifacts are tracked: "
            + ", ".join(tracked_python_artifacts)
        )

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

    if failures:
        raise SystemExit("repository consistency failed:\n- " + "\n- ".join(failures))
    print("repository consistency passed")


if __name__ == "__main__":
    main()
