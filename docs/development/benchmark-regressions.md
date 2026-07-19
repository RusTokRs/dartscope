---
id: doc://docs/development/benchmark-regressions.md
kind: development_policy
language: en
source_language: en
status: active
---

# Benchmark Regression Reporting

DartScope reports performance movement without treating an absolute hosted-runner duration as a
product contract. The permanent CI benchmark job is intentionally non-blocking and is not included
in the aggregate `dartscope/ci` status.

## Workloads

The checked-in harness measures four bounded release-mode workloads:

1. heuristic parsing over a generated declaration and invocation corpus;
2. project indexing over a generated 600-file import chain;
3. identifier-reference resolution over the same generated project;
4. `cargo package --workspace --no-verify` archive generation for all nine release crates.

The Rust harness emits deterministic work units and a digest with each duration. Parse, index, and
reference timings are compared only when baseline and candidate units and digests match. Aggregate
package archive size is retained as evidence but may change legitimately, so it does not suppress
package timing comparison.

## Same-Runner Comparison

For pull requests, CI compares the pull-request merge candidate with the exact base SHA. For pushes,
schedules, and manual runs, it compares the current commit with its recorded predecessor. Both trees
are built and measured on one runner. Seven paired samples are collected after warm-up, and baseline
versus candidate execution order alternates on every pair.

The report uses medians and median absolute deviation (MAD):

- noise above 15% on either side is `inconclusive (noisy)`;
- a `possible regression` requires at least a 20% median slowdown, the same paired-median slowdown,
  and slowdown support from at least 70% of alternating pairs;
- a symmetric sustained improvement is reported as `possible improvement`;
- all other comparable results are `stable`;
- changed deterministic work is `workload changed` rather than an invalid timing comparison.

These labels are review evidence, not pass/fail gates. Benchmark command failures remain visible in
the job and report artifact, while `continue-on-error` keeps runner or measurement instability from
blocking the Linux/Windows product matrix.

## Outputs And Review

Every run writes a Markdown step summary and uploads Markdown plus versioned JSON for 14 days. Review
a possible regression by reproducing it on a controlled machine, reducing the affected workload, and
checking deterministic counters before introducing any timing threshold. Promotion to a blocking gate
requires a workload-specific stable metric or a reviewed dedicated runner; shared hosted-runner wall
clock values are never sufficient.
