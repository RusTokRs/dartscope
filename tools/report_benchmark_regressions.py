#!/usr/bin/env python3
"""Compare DartScope benchmark workloads without absolute hosted-runner thresholds."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
import math
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import time
from typing import Callable, Iterable

WORKLOADS = ("parse", "index", "references", "package")
DEFAULT_SAMPLES = 7
REGRESSION_RATIO = 1.20
IMPROVEMENT_RATIO = 0.85
SUPPORT_RATIO = 1.10
NOISE_LIMIT = 0.15


@dataclass(frozen=True)
class Sample:
    elapsed_ns: int
    units: int
    digest: int


@dataclass(frozen=True)
class WorkloadReport:
    workload: str
    baseline_median_ns: int
    candidate_median_ns: int
    delta_percent: float
    baseline_noise: float
    candidate_noise: float
    paired_median_ratio: float
    supporting_pairs: int
    samples: int
    classification: str
    comparable: bool
    baseline_units: int
    candidate_units: int
    baseline_digest: int
    candidate_digest: int


class BenchmarkError(RuntimeError):
    pass


def median_absolute_deviation(values: Iterable[int]) -> float:
    values = list(values)
    median = statistics.median(values)
    return float(statistics.median(abs(value - median) for value in values))


def relative_noise(values: list[int]) -> float:
    median = float(statistics.median(values))
    if median == 0:
        return 0.0
    return median_absolute_deviation(values) / median


def classify_samples(
    baseline: list[int], candidate: list[int], *, comparable: bool = True
) -> tuple[str, float, int]:
    if len(baseline) != len(candidate) or not baseline:
        raise ValueError("baseline and candidate samples must have the same non-zero length")
    if not comparable:
        return "workload changed", 1.0, 0

    paired_ratios = [
        candidate_value / baseline_value
        for baseline_value, candidate_value in zip(baseline, candidate)
    ]
    paired_median = float(statistics.median(paired_ratios))
    baseline_noise = relative_noise(baseline)
    candidate_noise = relative_noise(candidate)
    if max(baseline_noise, candidate_noise) > NOISE_LIMIT:
        return "inconclusive (noisy)", paired_median, 0

    ratio = statistics.median(candidate) / statistics.median(baseline)
    supporting_regressions = sum(pair >= SUPPORT_RATIO for pair in paired_ratios)
    supporting_improvements = sum(
        pair <= 1.0 / SUPPORT_RATIO for pair in paired_ratios
    )
    required_support = math.ceil(len(paired_ratios) * 0.70)

    if (
        ratio >= REGRESSION_RATIO
        and paired_median >= REGRESSION_RATIO
        and supporting_regressions >= required_support
    ):
        return "possible regression", paired_median, supporting_regressions
    if (
        ratio <= IMPROVEMENT_RATIO
        and paired_median <= IMPROVEMENT_RATIO
        and supporting_improvements >= required_support
    ):
        return "possible improvement", paired_median, supporting_improvements
    return "stable", paired_median, max(
        supporting_regressions, supporting_improvements
    )


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode != 0:
        rendered = " ".join(command)
        raise BenchmarkError(f"command failed in {cwd}: {rendered}\n{completed.stdout}")
    return completed


def copy_harness(candidate: Path, baseline: Path) -> None:
    source = candidate / "crates/dartscope/examples/quality_benchmark.rs"
    destination = baseline / "crates/dartscope/examples/quality_benchmark.rs"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def build_harness(root: Path) -> Path:
    run_checked(
        [
            "cargo",
            "+1.95.0",
            "build",
            "-p",
            "dartscope",
            "--example",
            "quality_benchmark",
            "--release",
            "--locked",
        ],
        root,
    )
    return root / "target/release/examples/quality_benchmark"


def run_rust_workload(binary: Path, workload: str) -> Sample:
    completed = run_checked([str(binary), workload], binary.parents[3])
    try:
        payload = json.loads(completed.stdout.strip().splitlines()[-1])
    except (IndexError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        raise BenchmarkError(
            f"invalid {workload} benchmark output: {completed.stdout}"
        ) from error
    if payload.get("workload") != workload:
        raise BenchmarkError(
            f"benchmark workload mismatch: expected {workload}, got {payload!r}"
        )
    return Sample(
        elapsed_ns=int(payload["elapsed_ns"]),
        units=int(payload["units"]),
        digest=int(payload["digest"]),
    )


def run_package_workload(root: Path) -> Sample:
    package_dir = root / "target/package"
    shutil.rmtree(package_dir, ignore_errors=True)
    started = time.perf_counter_ns()
    run_checked(
        [
            "cargo",
            "+1.95.0",
            "package",
            "--workspace",
            "--allow-dirty",
            "--no-verify",
            "--locked",
        ],
        root,
    )
    elapsed = time.perf_counter_ns() - started
    archives = sorted(package_dir.glob("dartscope-*.crate"))
    if len(archives) != 9:
        raise BenchmarkError(
            f"cargo package created {len(archives)} DartScope archives instead of 9 in {package_dir}"
        )
    size = sum(archive.stat().st_size for archive in archives)
    return Sample(elapsed_ns=elapsed, units=size, digest=size)


def collect_paired_samples(
    baseline_runner: Callable[[], Sample],
    candidate_runner: Callable[[], Sample],
    sample_count: int,
) -> tuple[list[Sample], list[Sample]]:
    baseline_runner()
    candidate_runner()
    baseline: list[Sample] = []
    candidate: list[Sample] = []
    for index in range(sample_count):
        if index % 2 == 0:
            baseline.append(baseline_runner())
            candidate.append(candidate_runner())
        else:
            candidate.append(candidate_runner())
            baseline.append(baseline_runner())
    return baseline, candidate


def summarize(
    workload: str, baseline: list[Sample], candidate: list[Sample]
) -> WorkloadReport:
    baseline_times = [sample.elapsed_ns for sample in baseline]
    candidate_times = [sample.elapsed_ns for sample in candidate]
    baseline_units = int(statistics.median(sample.units for sample in baseline))
    candidate_units = int(statistics.median(sample.units for sample in candidate))
    baseline_digest = int(statistics.median(sample.digest for sample in baseline))
    candidate_digest = int(statistics.median(sample.digest for sample in candidate))
    comparable = workload == "package" or (
        baseline_units == candidate_units and baseline_digest == candidate_digest
    )
    classification, paired_median, support = classify_samples(
        baseline_times, candidate_times, comparable=comparable
    )
    baseline_median = int(statistics.median(baseline_times))
    candidate_median = int(statistics.median(candidate_times))
    delta = (candidate_median / baseline_median - 1.0) * 100.0
    return WorkloadReport(
        workload=workload,
        baseline_median_ns=baseline_median,
        candidate_median_ns=candidate_median,
        delta_percent=delta,
        baseline_noise=relative_noise(baseline_times),
        candidate_noise=relative_noise(candidate_times),
        paired_median_ratio=paired_median,
        supporting_pairs=support,
        samples=len(baseline),
        classification=classification,
        comparable=comparable,
        baseline_units=baseline_units,
        candidate_units=candidate_units,
        baseline_digest=baseline_digest,
        candidate_digest=candidate_digest,
    )


def git_sha(root: Path) -> str:
    return run_checked(["git", "rev-parse", "HEAD"], root).stdout.strip()


def render_markdown(
    baseline_sha: str, candidate_sha: str, reports: list[WorkloadReport]
) -> str:
    lines = [
        "# DartScope benchmark regression report",
        "",
        f"- baseline: `{baseline_sha}`",
        f"- candidate: `{candidate_sha}`",
        f"- paired samples per workload: `{reports[0].samples if reports else 0}`",
        "",
        "> Timing classifications are informational. The job compares base and head on the same runner,",
        "> alternates execution order, and uses medians plus median absolute deviation. No absolute",
        "> hosted-runner duration and no measured slowdown can fail the blocking `dartscope/ci` status.",
        "",
        "| Workload | Baseline median | Candidate median | Delta | Noise (base/head) | Classification |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    for report in reports:
        lines.append(
            "| {workload} | {baseline:.2f} ms | {candidate:.2f} ms | {delta:+.1f}% | {base_noise:.1f}% / {candidate_noise:.1f}% | {classification} |".format(
                workload=report.workload,
                baseline=report.baseline_median_ns / 1_000_000,
                candidate=report.candidate_median_ns / 1_000_000,
                delta=report.delta_percent,
                base_noise=report.baseline_noise * 100,
                candidate_noise=report.candidate_noise * 100,
                classification=report.classification,
            )
        )
    lines.extend(
        [
            "",
            "A `possible regression` requires at least a 20% median slowdown, a matching paired median,",
            "support from at least 70% of alternating pairs, and MAD noise no greater than 15% on either side.",
            "A changed deterministic workload digest is reported as `workload changed` instead of comparing",
            "unlike work. Package archive size is recorded but does not suppress package timing comparison.",
            "",
        ]
    )
    return "\n".join(lines)


def write_failure(markdown_path: Path, json_path: Path, error: Exception) -> None:
    message = str(error)
    markdown_path.write_text(
        "# DartScope benchmark regression report\n\n"
        "The non-blocking benchmark reporter could not complete.\n\n"
        f"```text\n{message}\n```\n",
        encoding="utf-8",
    )
    json_path.write_text(
        json.dumps({"schema_version": 1, "status": "error", "error": message}, indent=2)
        + "\n",
        encoding="utf-8",
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--markdown", type=Path, required=True)
    parser.add_argument("--json", dest="json_path", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.samples < 3 or args.samples % 2 == 0:
        raise SystemExit("--samples must be an odd integer greater than or equal to 3")
    baseline = args.baseline.resolve()
    candidate = args.candidate.resolve()
    args.markdown.parent.mkdir(parents=True, exist_ok=True)
    args.json_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        copy_harness(candidate, baseline)
        baseline_binary = build_harness(baseline)
        candidate_binary = build_harness(candidate)
        reports: list[WorkloadReport] = []
        for workload in WORKLOADS:
            if workload == "package":
                baseline_runner = lambda: run_package_workload(baseline)
                candidate_runner = lambda: run_package_workload(candidate)
            else:
                baseline_runner = lambda workload=workload: run_rust_workload(
                    baseline_binary, workload
                )
                candidate_runner = lambda workload=workload: run_rust_workload(
                    candidate_binary, workload
                )
            baseline_samples, candidate_samples = collect_paired_samples(
                baseline_runner, candidate_runner, args.samples
            )
            reports.append(summarize(workload, baseline_samples, candidate_samples))

        baseline_sha = git_sha(baseline)
        candidate_sha = git_sha(candidate)
        args.markdown.write_text(
            render_markdown(baseline_sha, candidate_sha, reports), encoding="utf-8"
        )
        args.json_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "status": "ok",
                    "baseline_sha": baseline_sha,
                    "candidate_sha": candidate_sha,
                    "samples": args.samples,
                    "reports": [asdict(report) for report in reports],
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        return 0
    except (BenchmarkError, OSError, subprocess.SubprocessError) as error:
        write_failure(args.markdown, args.json_path, error)
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
