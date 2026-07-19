from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "report_benchmark_regressions.py"
SPEC = importlib.util.spec_from_file_location("benchmark_regressions", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class BenchmarkRegressionTests(unittest.TestCase):
    def test_sustained_relative_slowdown_is_reported(self) -> None:
        classification, paired_median, support = MODULE.classify_samples(
            [100, 101, 99, 100, 102, 98, 100],
            [128, 130, 126, 127, 131, 125, 129],
        )
        self.assertEqual(classification, "possible regression")
        self.assertGreaterEqual(paired_median, 1.20)
        self.assertGreaterEqual(support, 5)

    def test_single_outlier_does_not_create_a_regression(self) -> None:
        classification, _, _ = MODULE.classify_samples(
            [100, 100, 100, 100, 100, 100, 100],
            [100, 101, 99, 100, 300, 100, 99],
        )
        self.assertEqual(classification, "stable")

    def test_high_median_absolute_deviation_is_inconclusive(self) -> None:
        classification, _, _ = MODULE.classify_samples(
            [70, 90, 100, 110, 130, 150, 170],
            [100, 130, 150, 180, 210, 250, 300],
        )
        self.assertEqual(classification, "inconclusive (noisy)")

    def test_changed_workload_shape_is_not_compared(self) -> None:
        classification, paired_median, support = MODULE.classify_samples(
            [100, 100, 100], [200, 200, 200], comparable=False
        )
        self.assertEqual(classification, "workload changed")
        self.assertEqual(paired_median, 1.0)
        self.assertEqual(support, 0)

    def test_markdown_states_non_blocking_policy(self) -> None:
        report = MODULE.WorkloadReport(
            workload="parse",
            baseline_median_ns=1_000_000,
            candidate_median_ns=1_100_000,
            delta_percent=10.0,
            baseline_noise=0.01,
            candidate_noise=0.02,
            paired_median_ratio=1.1,
            supporting_pairs=2,
            samples=7,
            classification="stable",
            comparable=True,
            baseline_units=10,
            candidate_units=10,
            baseline_digest=20,
            candidate_digest=20,
        )
        markdown = MODULE.render_markdown("base", "head", [report])
        self.assertIn("No absolute", markdown)
        self.assertIn("no measured slowdown can fail", markdown)
        self.assertIn("| parse |", markdown)


if __name__ == "__main__":
    unittest.main()
