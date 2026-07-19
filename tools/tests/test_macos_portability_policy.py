from __future__ import annotations

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / ".github" / "workflows" / "ci.yml"
POLICY = ROOT / "docs" / "development" / "macos-portability.md"


class MacosPortabilityPolicyTests(unittest.TestCase):
    def test_signal_is_pinned_and_non_blocking(self) -> None:
        text = CI.read_text(encoding="utf-8")
        match = re.search(
            r"(?ms)^  macos_portability:\n(?P<body>.*?)(?=^  [a-zA-Z0-9_]+:|\Z)", text
        )
        self.assertIsNotNone(match)
        body = match.group("body")
        self.assertIn("continue-on-error: true", body)
        self.assertIn("runs-on: macos-15", body)
        self.assertIn("timeout-minutes: 30", body)
        self.assertIn("cargo +1.95.0 check --workspace --all-targets --locked", body)
        self.assertIn("cargo +1.95.0 test --workspace --locked --quiet", body)
        self.assertIn("cargo +1.95.0 package --workspace", body)
        self.assertIn('test "$archive_count" = "9"', body)

    def test_signal_is_excluded_from_aggregate_status(self) -> None:
        text = CI.read_text(encoding="utf-8")
        report = re.search(
            r"(?ms)^  report:\n(?P<body>.*?)(?=^  [a-zA-Z0-9_]+:|\Z)", text
        )
        self.assertIsNotNone(report)
        body = report.group("body")
        self.assertNotIn("macos_portability", body)
        self.assertNotIn("benchmark_report", body)

    def test_promotion_criteria_are_explicit(self) -> None:
        text = POLICY.read_text(encoding="utf-8")
        for fragment in (
            "at least 30 valid",
            "at least six weeks",
            "at least 95%",
            "no unresolved reproducible macOS-only product defect",
            "continue-on-error` removed",
            "runner-image migration restarts the observation window",
        ):
            self.assertIn(fragment, text)


if __name__ == "__main__":
    unittest.main()
