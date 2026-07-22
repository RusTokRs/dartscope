from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "check-workflow-policy.py"
SPEC = importlib.util.spec_from_file_location("workflow_policy", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
POLICY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = POLICY
SPEC.loader.exec_module(POLICY)


# Policy fixtures intentionally model both release-quality jobs as blocking.
class WorkflowPolicyTests(unittest.TestCase):
    def make_root(self, ci: str, release: str) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        workflows = root / ".github" / "workflows"
        workflows.mkdir(parents=True)
        (workflows / "ci.yml").write_text(ci, encoding="utf-8")
        (workflows / "release.yml").write_text(release, encoding="utf-8")
        return root

    def valid_ci(self) -> str:
        return f"""name: CI
on: [push]
permissions:
  contents: read
jobs:
  policy:
    permissions:
      contents: read
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{POLICY.REVIEWED_ACTIONS['actions/checkout'][0]} # v6.0.2
      - run: go install github.com/rhysd/actionlint/cmd/actionlint@{POLICY.ACTIONLINT_VERSION}
  benchmark_report:
    runs-on: ubuntu-latest
    steps:
      - run: echo benchmark
  macos_portability:
    runs-on: macos-15
    steps:
      - run: echo macos
  report:
    needs: [benchmark_report, macos_portability]
    if: always() && github.event_name != 'pull_request'
    permissions:
      statuses: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/github-script@{POLICY.REVIEWED_ACTIONS['actions/github-script'][0]} # v9.0.0
        with:
          script: |
            const results = [
              '${{{{ needs.benchmark_report.result }}}}',
              '${{{{ needs.macos_portability.result }}}}',
            ];
"""

    def valid_release(self) -> str:
        return f"""name: Release
on: [workflow_dispatch]
permissions:
  contents: read
jobs:
  package:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{POLICY.REVIEWED_ACTIONS['actions/checkout'][0]} # v6.0.2
      - run: go install github.com/rhysd/actionlint/cmd/actionlint@{POLICY.ACTIONLINT_VERSION}
      - uses: actions/upload-artifact@{POLICY.REVIEWED_ACTIONS['actions/upload-artifact'][0]} # v7.0.1
  publish:
    if: github.event_name == 'workflow_dispatch' && startsWith(github.ref, 'refs/tags/v')
    environment: crates-io
    runs-on: ubuntu-latest
    steps:
      - run: echo "$CARGO_REGISTRY_TOKEN"
        env:
          CARGO_REGISTRY_TOKEN: ${{{{ secrets.CARGO_REGISTRY_TOKEN }}}}
"""

    def messages(self, root: Path) -> str:
        return "\n".join(failure.render() for failure in POLICY.check_workflows(root))

    def test_accepts_reviewed_inventory(self) -> None:
        root = self.make_root(self.valid_ci(), self.valid_release())
        self.assertEqual(POLICY.check_workflows(root), [])

    def test_rejects_mutable_list_form_action(self) -> None:
        root = self.make_root(
            self.valid_ci().replace(
                f"actions/checkout@{POLICY.REVIEWED_ACTIONS['actions/checkout'][0]} # v6.0.2",
                "actions/checkout@v6",
            ),
            self.valid_release(),
        )
        self.assertIn("must use reviewed SHA", self.messages(root))

    def test_rejects_missing_release_comment(self) -> None:
        root = self.make_root(
            self.valid_ci().replace(" # v9.0.0", ""), self.valid_release()
        )
        self.assertIn("adjacent release comment", self.messages(root))

    def test_rejects_unknown_permission_and_pull_request_target(self) -> None:
        root = self.make_root(
            self.valid_ci().replace("on: [push]", "on:\n  pull_request_target:")
            .replace("contents: read", "mystery: read", 1),
            self.valid_release(),
        )
        messages = self.messages(root)
        self.assertIn("unknown workflow permission", messages)
        self.assertIn("pull_request_target is forbidden", messages)

    def test_rejects_unreviewed_write_permission(self) -> None:
        root = self.make_root(
            self.valid_ci().replace("contents: read", "contents: write", 1),
            self.valid_release(),
        )
        self.assertIn("unreviewed write permissions", self.messages(root))

    def test_requires_push_only_status_reporter(self) -> None:
        root = self.make_root(
            self.valid_ci().replace(" && github.event_name != 'pull_request'", ""),
            self.valid_release(),
        )
        self.assertIn("confined to a job skipped", self.messages(root))

    def test_requires_release_quality_jobs_to_remain_blocking(self) -> None:
        ci = self.valid_ci().replace(
            "  benchmark_report:\n    runs-on:",
            "  benchmark_report:\n    continue-on-error: true\n    runs-on:",
        )
        root = self.make_root(ci, self.valid_release())

        self.assertIn("must remain blocking", self.messages(root))

    def test_requires_release_quality_jobs_in_aggregate_status(self) -> None:
        ci = self.valid_ci().replace(
            "${{ needs.macos_portability.result }}",
            "success",
        )
        root = self.make_root(ci, self.valid_release())

        self.assertIn("aggregate status must include", self.messages(root))


if __name__ == "__main__":
    unittest.main()
