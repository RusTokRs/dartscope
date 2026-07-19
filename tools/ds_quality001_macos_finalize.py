#!/usr/bin/env python3
from pathlib import Path

path = Path("docs/development/dartscope-library-plan.md")
text = path.read_text(encoding="utf-8")
replacements = [
    (
        "Status: in progress. Priority: P1. Prerequisite: DS-AUDIT-001.\n",
        "Status: verified. Priority: P1. Prerequisite: DS-AUDIT-001.\n",
    ),
    (
        """10. Added non-blocking same-runner benchmark regression reporting for parsing, project indexing,
    identifier-reference resolution, and package archive generation. Base/head execution order alternates,
    reports use medians and MAD plus sustained relative evidence, and Markdown/JSON artifacts remain
    informational rather than adding an absolute hosted-runner timing gate.
""",
        """10. Added non-blocking same-runner benchmark regression reporting for parsing, project indexing,
    identifier-reference resolution, and package archive generation. Base/head execution order alternates,
    reports use medians and MAD plus sustained relative evidence, and Markdown/JSON artifacts remain
    informational rather than adding an absolute hosted-runner timing gate.
11. Added a pinned `macos-15` arm64 portability signal that records runner/compiler evidence, checks all
    workspace targets, runs the complete workspace test suite, and verifies all nine package archives.
    Workflow policy and the full signal passed in pull-request CI run `29698405538`; the job remains
    non-blocking and excluded from the aggregate `dartscope/ci` status during its observation window.
""",
    ),
    (
        """- Benchmark timing classifications are non-blocking and excluded from `dartscope/ci`. A changed
  deterministic workload digest suppresses unlike timing comparisons, and promotion to a blocking gate
  requires a stable workload-specific metric or reviewed dedicated runner.
""",
        """- Benchmark timing classifications are non-blocking and excluded from `dartscope/ci`. A changed
  deterministic workload digest suppresses unlike timing comparisons, and promotion to a blocking gate
  requires a stable workload-specific metric or reviewed dedicated runner.
- The verified macOS signal is also excluded from `dartscope/ci`. Promotion requires a GA pinned runner,
  at least 30 valid default-branch or scheduled observations across six weeks, at least 95% valid-run
  success, and no unresolved macOS-only product defect or platform-specific bypass.
""",
    ),
    (
        """5. Add macOS as a non-blocking portability signal for the `0.2` cycle and define promotion criteria.
""",
        """5. Observe the verified pinned macOS portability signal for the `0.2` cycle and promote it only
   through the reviewed criteria in `docs/development/macos-portability.md`.
""",
    ),
    (
        """- benchmarks report regressions without relying on unstable absolute hosted-runner timings;
- every exception has an owner, rationale, and review date.
""",
        """- benchmarks report regressions without relying on unstable absolute hosted-runner timings;
- macOS portability failures remain visible without participating in the blocking aggregate status;
- every exception has an owner, rationale, and review date.
""",
    ),
    (
        """Continue `DS-QUALITY-001` with the non-blocking macOS portability signal for the `0.2` cycle and
record explicit promotion criteria. Security, bounded fuzzing, deterministic properties, and relative
benchmark regression reporting are now present; `DS-QUALITY-001` remains `in_progress` until the macOS
signal is verified. `DS-COMPAT-001` remains research.
""",
        """Implement `DS-INDEX-006` next. The durable quality gate is now verified across dependency policy,
bounded fuzzing, deterministic properties, relative benchmark reporting, and a non-blocking macOS
portability signal. Broader references and lexical scope resolution are the next P1 prerequisite for
language-server work; `DS-COMPAT-001` remains research.
""",
    ),
]
for old, new in replacements:
    if text.count(old) != 1:
        raise SystemExit(f"roadmap anchor did not match exactly once: {old[:100]!r}")
    text = text.replace(old, new)
path.write_text(text, encoding="utf-8")
