#!/usr/bin/env python3
"""Apply the reviewed DS-QUALITY-001 dependency gate with final lock and warning fixes."""

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = "b968cb690b930715fe79c37536010cc7c1f3ed1b"
SOURCE_PATH = "tools/ds_quality001_dependency_gates_apply_v3.py"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"{path.relative_to(ROOT)}: expected one post-patch anchor, found {count}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


source = subprocess.check_output(
    ["git", "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
    cwd=ROOT,
    text=True,
)
raw_checker_replacement = """replacements = [
    (
        "CHECKER.write_text('''#!/usr/bin/env python3",
        "CHECKER.write_text(r'''#!/usr/bin/env python3",
    ),
"""
if source.count("replacements = [\n") != 1:
    raise SystemExit("reviewed v3 replacement-list anchor drifted")
source = source.replace("replacements = [\n", raw_checker_replacement, 1)
virtual_path = ROOT / SOURCE_PATH
exec(
    compile(source, str(virtual_path), "exec"),
    {"__name__": "__main__", "__file__": str(virtual_path)},
)

replace_once(
    ROOT / ".cargo/audit.toml",
    """[output]
deny = ["unmaintained"]
show_tree = true

[yanked]
""",
    """[output]
deny = ["unmaintained"]
show_tree = true
quiet = false

[yanked]
""",
)

replace_once(
    ROOT / "crates/dartscope-parse/Cargo.toml",
    """dartscope-resolve.workspace = true
serde.workspace = true
yaml-rust2.workspace = true
""",
    """dartscope-resolve.workspace = true
yaml-rust2.workspace = true
""",
)

roadmap = ROOT / "docs/development/dartscope-library-plan.md"
replace_once(
    roadmap,
    """### DS-CLI-003: Lint Command, Configuration, And SARIF

Status: implemented. Priority: P1. Prerequisite: DS-LINT-001.
""",
    """### DS-CLI-003: Lint Command, Configuration, And SARIF

Status: verified. Priority: P1. Prerequisite: DS-LINT-001.
""",
)
replace_once(
    roadmap,
    """- **Verification pending:** the Rust 1.95 Ubuntu feature finalizer and focused cleanup gate passed, but
  GitHub did not publish the permanent Linux/Windows aggregate status for the final clean main SHA.
  Promote this task to `verified` only after a later clean main SHA reports `dartscope/ci` success.
""",
    """- **Verification completed (2026-07-18):** the later clean DS-INDEX-005 feature SHA
  `5b1f82eacb606e5692fddd040e1f8dc465989e6b` reported aggregate `dartscope/ci: success`, covering the
  permanent Rust 1.95 Linux/Windows matrix required by this task.
""",
)
replace_once(
    roadmap,
    """- a clean permanent hosted Linux/Windows matrix reports aggregate `dartscope/ci` success before the
  task is promoted from `implemented` to `verified`.
""",
    """- a later clean permanent hosted Linux/Windows matrix reports aggregate `dartscope/ci` success.
""",
)
replace_once(
    roadmap,
    """3. Added focused policy tests for missing metadata, expired exceptions, native-ignore drift, and a valid
   reviewed unused-dependency exception. The initial exception registry is empty.

Findings and limits:
""",
    """3. Added focused policy tests for missing metadata, expired exceptions, native-ignore drift, and a valid
   reviewed unused-dependency exception. The initial exception registry is empty.
4. **P1 fixed:** `dartscope-parse` declared `serde` directly without using it. The dependency and its
   stale package-level lock edge were removed rather than hidden behind a cargo-machete exception.
5. **P2 fixed:** the first generated dependency-policy checker used a non-raw outer Python string and
   emitted an invalid-escape `SyntaxWarning`; generation is now warning-clean under `-W error`.

Findings and limits:
""",
)

replace_once(
    ROOT / "docs/development/dependency-quality.md",
    """## Maintenance Boundary

Tool versions are duplicated deliberately in CI and the policy file; `check-dependency-policy.py` rejects
""",
    """## Maintenance Boundary

The initial unused-dependency scan found a real direct `serde` declaration in `dartscope-parse` with no
crate-local use. It and the stale package-level lock edge were removed instead of allowlisted; exceptions
are reserved for reviewed false positives. Generated policy code is tested with Python syntax warnings
promoted to errors so regex escapes cannot regress silently.

Tool versions are duplicated deliberately in CI and the policy file; `check-dependency-policy.py` rejects
""",
)

replace_once(
    ROOT / "CHANGELOG.md",
    """### Fixed

- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
""",
    """### Fixed

- Removed an unused direct `serde` dependency and stale lock edge from `dartscope-parse` instead of
  suppressing the unused-dependency gate.
- Incremental reference caches now invalidate same-name `NotVisible` evidence and sibling-part
""",
)

for relative in (
    "docs/development/ds-quality001-machete-diagnostic.md",
    "docs/development/ds-quality001-dependency-failure.md",
    "docs/development/ds-quality001-dependency-v5-failure.md",
    "docs/development/ds-quality001-policy-test-diagnostic.md",
    "docs/development/ds-quality001-rustsec-diagnostic.md",
    "docs/development/ds-quality001-v5-compile-diagnostic.md",
    "docs/development/ds-index005-dependency-fingerprint-anchor-diagnostic.md",
    "docs/development/ds-index005-dependency-fingerprint-diagnostic.md",
):
    (ROOT / relative).unlink(missing_ok=True)

print("DS-QUALITY-001 dependency gate v6 slice applied")
