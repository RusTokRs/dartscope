#!/usr/bin/env python3
"""Enforce the reviewed GitHub Actions supply-chain and permission policy."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_DIR = ROOT / ".github" / "workflows"

REVIEWED_ACTIONS = {
    "actions/checkout": (
        "de0fac2e4500dabe0009e67214ff5f5447ce83dd",
        "v6.0.2",
    ),
    "actions/github-script": (
        "3a2844b7e9c422d3c10d287c895573f7108da1b3",
        "v9.0.0",
    ),
    "actions/upload-artifact": (
        "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "v7.0.1",
    ),
}
ACTIONLINT_VERSION = "v1.7.12"
KNOWN_PERMISSIONS = {
    "actions",
    "attestations",
    "checks",
    "code-quality",
    "contents",
    "deployments",
    "discussions",
    "id-token",
    "issues",
    "models",
    "packages",
    "pages",
    "pull-requests",
    "security-events",
    "statuses",
    "vulnerability-alerts",
}
ALLOWED_WRITE_PERMISSIONS = {
    Path(".github/workflows/ci.yml"): {"statuses"},
    Path(".github/workflows/release.yml"): set(),
}
USES_PATTERN = re.compile(
    r"^\s*(?:-\s*)?uses:\s*(?P<target>[^#\s]+)(?:\s+#\s*(?P<comment>.+?))?\s*$"
)
PERMISSION_PATTERN = re.compile(r"^(?P<indent>\s*)permissions:\s*(?P<inline>[^#]*)")
PERMISSION_ENTRY_PATTERN = re.compile(
    r"^(?P<indent>\s*)(?P<name>[a-z-]+):\s*(?P<value>[a-z-]+)\s*(?:#.*)?$"
)


@dataclass(frozen=True)
class PolicyFailure:
    path: Path
    line: int
    message: str

    def render(self) -> str:
        location = f"{self.path}:{self.line}" if self.line else str(self.path)
        return f"{location}: {self.message}"


def workflow_paths(root: Path) -> list[Path]:
    directory = root / ".github" / "workflows"
    return sorted({*directory.glob("*.yml"), *directory.glob("*.yaml")})


def strip_yaml_comment(line: str) -> str:
    quote: str | None = None
    escaped = False
    for index, char in enumerate(line):
        if quote is not None:
            if escaped:
                escaped = False
            elif quote == '"' and char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in {"'", '"'}:
            quote = char
        elif char == "#":
            return line[:index]
    return line


def check_action_references(path: Path, lines: list[str]) -> list[PolicyFailure]:
    failures: list[PolicyFailure] = []
    for number, line in enumerate(lines, 1):
        match = USES_PATTERN.match(line)
        if match is None:
            continue
        target = match.group("target")
        if target.startswith("./"):
            continue
        if target.startswith("docker://"):
            if "@sha256:" not in target:
                failures.append(
                    PolicyFailure(path, number, "container actions must use an immutable sha256 digest")
                )
            continue
        if "@" not in target:
            failures.append(PolicyFailure(path, number, "Action reference is missing @<commit>"))
            continue
        action, reference = target.rsplit("@", 1)
        reviewed = REVIEWED_ACTIONS.get(action)
        if reviewed is None:
            failures.append(
                PolicyFailure(path, number, f"Action {action!r} is not in REVIEWED_ACTIONS")
            )
            continue
        expected_sha, expected_tag = reviewed
        if reference != expected_sha:
            failures.append(
                PolicyFailure(
                    path,
                    number,
                    f"Action {action!r} must use reviewed SHA {expected_sha}",
                )
            )
        comment = (match.group("comment") or "").strip()
        if comment != expected_tag:
            failures.append(
                PolicyFailure(
                    path,
                    number,
                    f"Action {action!r} must retain adjacent release comment '# {expected_tag}'",
                )
            )
    return failures


def permission_blocks(path: Path, lines: list[str]) -> tuple[list[tuple[int, dict[str, str]]], list[PolicyFailure]]:
    blocks: list[tuple[int, dict[str, str]]] = []
    failures: list[PolicyFailure] = []
    for index, line in enumerate(lines):
        match = PERMISSION_PATTERN.match(strip_yaml_comment(line).rstrip())
        if match is None:
            continue
        base_indent = len(match.group("indent"))
        inline = match.group("inline").strip()
        number = index + 1
        if inline:
            failures.append(
                PolicyFailure(path, number, "permissions must use an explicit mapping, not read-all/write-all")
            )
            continue
        entries: dict[str, str] = {}
        cursor = index + 1
        while cursor < len(lines):
            raw = lines[cursor]
            clean = strip_yaml_comment(raw).rstrip()
            if not clean.strip():
                cursor += 1
                continue
            indent = len(clean) - len(clean.lstrip())
            if indent <= base_indent:
                break
            entry = PERMISSION_ENTRY_PATTERN.match(clean)
            if entry is None:
                failures.append(
                    PolicyFailure(path, cursor + 1, "invalid permissions mapping entry")
                )
                cursor += 1
                continue
            name = entry.group("name")
            value = entry.group("value")
            if name not in KNOWN_PERMISSIONS:
                failures.append(
                    PolicyFailure(path, cursor + 1, f"unknown workflow permission {name!r}")
                )
            if value not in {"read", "write", "none"}:
                failures.append(
                    PolicyFailure(path, cursor + 1, f"invalid permission value {value!r}")
                )
            entries[name] = value
            cursor += 1
        if not entries:
            failures.append(PolicyFailure(path, number, "permissions mapping must not be empty"))
        blocks.append((base_indent, entries))
    return blocks, failures


def check_permissions(root: Path, path: Path, lines: list[str]) -> list[PolicyFailure]:
    relative = path.relative_to(root)
    blocks, failures = permission_blocks(relative, lines)
    if not any(indent == 0 for indent, _ in blocks):
        failures.append(PolicyFailure(relative, 0, "workflow must declare top-level permissions"))
    allowed_writes = ALLOWED_WRITE_PERMISSIONS.get(relative)
    if allowed_writes is None:
        failures.append(
            PolicyFailure(relative, 0, "workflow is not registered in the write-permission allowlist")
        )
        allowed_writes = set()
    writes = {
        name
        for _, entries in blocks
        for name, value in entries.items()
        if value == "write"
    }
    unexpected = writes - allowed_writes
    if unexpected:
        failures.append(
            PolicyFailure(relative, 0, f"unreviewed write permissions: {sorted(unexpected)}")
        )
    if relative == Path(".github/workflows/ci.yml") and "statuses" in writes:
        text = "\n".join(lines)
        if "github.event_name != 'pull_request'" not in text:
            failures.append(
                PolicyFailure(
                    relative,
                    0,
                    "statuses: write must be confined to a job skipped for pull_request events",
                )
            )
    return failures


def check_events(path: Path, lines: list[str]) -> list[PolicyFailure]:
    failures: list[PolicyFailure] = []
    for number, line in enumerate(lines, 1):
        clean = strip_yaml_comment(line).strip()
        if clean.startswith("pull_request_target:"):
            failures.append(
                PolicyFailure(path, number, "pull_request_target is forbidden without a reviewed policy change")
            )
    return failures


def check_release_boundary(root: Path) -> list[PolicyFailure]:
    path = root / ".github" / "workflows" / "release.yml"
    relative = path.relative_to(root)
    text = path.read_text(encoding="utf-8")
    required = {
        "environment: crates-io": "publish job must retain the protected crates-io environment",
        "github.event_name == 'workflow_dispatch'": "publishing must require workflow_dispatch",
        "startsWith(github.ref, 'refs/tags/v')": "publishing must require an exact version tag ref",
        "CARGO_REGISTRY_TOKEN": "publishing must receive the crates.io token only in the publish step",
    }
    return [
        PolicyFailure(relative, 0, message)
        for fragment, message in required.items()
        if fragment not in text
    ]


def check_actionlint_pin(root: Path) -> list[PolicyFailure]:
    failures: list[PolicyFailure] = []
    needle = f"github.com/rhysd/actionlint/cmd/actionlint@{ACTIONLINT_VERSION}"
    for relative in (
        Path(".github/workflows/ci.yml"),
        Path(".github/workflows/release.yml"),
    ):
        if needle not in (root / relative).read_text(encoding="utf-8"):
            failures.append(
                PolicyFailure(relative, 0, f"workflow must install actionlint {ACTIONLINT_VERSION}")
            )
    return failures


def check_workflows(root: Path = ROOT) -> list[PolicyFailure]:
    failures: list[PolicyFailure] = []
    paths = workflow_paths(root)
    expected = set(ALLOWED_WRITE_PERMISSIONS)
    actual = {path.relative_to(root) for path in paths}
    if actual != expected:
        failures.append(
            PolicyFailure(
                Path(".github/workflows"),
                0,
                f"workflow inventory changed: expected={sorted(map(str, expected))}, actual={sorted(map(str, actual))}",
            )
        )
    for path in paths:
        relative = path.relative_to(root)
        lines = path.read_text(encoding="utf-8").splitlines()
        failures.extend(check_action_references(relative, lines))
        failures.extend(check_permissions(root, path, lines))
        failures.extend(check_events(relative, lines))
    failures.extend(check_release_boundary(root))
    failures.extend(check_actionlint_pin(root))
    return failures


def main() -> None:
    failures = check_workflows()
    if failures:
        for failure in failures:
            print(f"error: {failure.render()}", file=sys.stderr)
        raise SystemExit(1)
    print("workflow policy passed")


if __name__ == "__main__":
    main()
