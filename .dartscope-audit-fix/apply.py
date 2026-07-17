from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")


baseline_old = """## Verified Baseline

Baseline reviewed on 2026-07-16.

| Area | Status | Evidence in repository |
| --- | --- | --- |
| Rust workspace and eight crates | verified | root `Cargo.toml`; exact Rust 1.95.0 Linux/Windows quality, test, edition, and feature matrix passed |
| Core normalized model | implemented | declarations, generic invocations, spans, diagnostics, and compatibility projections; pre-1.0 migration work remains |
| File and pubspec analysis | in_progress | heuristic declarations and generic invocations plus marked `yaml-rust2` pubspec backend; unit and project fixtures |
| Package config v2 and package URI resolution | in_progress | `dartscope-resolve`, six resolver tests |
| URI graph, parts, and GraphQL linking | in_progress | `dartscope-index`, deterministic JSON contract tests |
| Flutter project inventory | verified | optional convention derivation and deterministic inventory behind the `flutter` feature |
| Versioned JSON contracts | verified | seven named v1 command envelopes, golden fixtures, and migration policy |
| CLI process contract | verified | help, version, exit codes, deterministic discovery, and Linux/Windows process tests |
| Hosted CI | verified | Rust 1.95.0 quality, Linux/Windows tests, edition-2024, and umbrella feature matrix publish granular and aggregate statuses |
| Contributor and agent workflow | verified | `AGENTS.md`, `CONTRIBUTING.md`, Rust code standard |
| Lint/rule engine | planned | crate not created |
| Parser backend port | verified | `DartParser` capability contract, default heuristic backend, injection path, and backend documentation |
"""
baseline_new = """## Verified Baseline

Baseline reviewed on 2026-07-17 after a full repository, package, release, dependency, and
cross-platform audit.

| Area | Status | Evidence in repository |
| --- | --- | --- |
| Rust workspace and nine crates | verified | root `Cargo.toml`; exact Rust 1.95.0 Linux/Windows quality, test, edition, and feature matrix passed |
| Core normalized model | implemented | declarations, generic invocations, spans, diagnostics, and compatibility projections; pre-1.0 migration work remains |
| File and pubspec analysis | in_progress | heuristic declarations and generic invocations plus marked `yaml-rust2` pubspec backend; unit and project fixtures |
| Package config v2 and package URI resolution | in_progress | `dartscope-resolve`, resolver fixtures, and project URI integration tests |
| URI graph, parts, GraphQL, and namespace linking | verified for current model | deterministic index, reference-resolution, part-link, and JSON contract tests |
| Flutter project inventory | verified | optional convention derivation and deterministic inventory behind the `flutter` feature |
| Versioned JSON contracts | verified | seven named v1 command envelopes, golden fixtures, and migration policy |
| CLI process contract | verified | help, version, exit codes, deterministic discovery, and Linux/Windows process tests |
| Hosted CI | verified | Rust 1.95.0 quality, Linux/Windows tests, edition-2024, and umbrella feature matrix publish granular and aggregate statuses |
| Contributor and agent workflow | verified | `AGENTS.md`, `CONTRIBUTING.md`, Rust code standard |
| Lint/rule engine | verified | optional `dartscope-lints`, five deterministic rules, stable IDs, severity overrides, and focused fixtures |
| Release packaging | verified with audit corrections | nine `.crate` archives, publish topology, release policy, and protected manual publishing path |
| Parser backend port | verified | `DartParser` capability contract, default heuristic backend, injection path, and backend documentation |
"""
replace_once("docs/development/dartscope-library-plan.md", baseline_old, baseline_new)

behaviors_anchor = """- optional Flutter convention derivation and deterministic inventory that preserve route path
  kind, confidence, paths, spans, and ordering.

## Known Architectural Debt
"""
audit_section = """- optional Flutter convention derivation and deterministic inventory that preserve route path
  kind, confidence, paths, spans, and ordering.

## Full Repository Audit (2026-07-17)

The audit rechecked the repository from workspace metadata through release execution rather than
assuming previously green slices were still mutually consistent.

Audit scope:

- exact Rust 1.95.0 formatting, workspace/all-feature checks, all-target tests, Clippy, and rustdoc;
- isolated umbrella features plus the normal Ubuntu and Windows hosted matrices;
- all nine release archives, normalized packaged manifests, README inclusion, and publish topology;
- RustSec advisory and unused-dependency scans, duplicate dependency reporting, script syntax, and
  repository metadata consistency;
- release tag/changelog state, executable file modes, workflow invocation behavior, and third-party
  Action references.

Confirmed results:

- source, tests, Clippy, rustdoc, isolated features, and package archives passed;
- a transient Windows test failure on an intermediate audit commit did not reproduce on the clean
  audit head, where the complete standard Ubuntu/Windows matrix passed;
- no production Rust or command-contract regression was reproduced by the audit.

Findings and disposition:

1. **P0 fixed:** `tools/publish-crates.sh` was stored as mode `100644` while the release workflow
   executed it directly. The audited release path now invokes it through `bash`, preserves executable
   mode, and checks both conditions permanently.
2. **P1 fixed:** `CHANGELOG.md` described `0.1.0` as released even though tag `v0.1.0` did not exist.
   Release notes remain under `Unreleased` until the exact tag is created.
3. **P1 fixed:** the verified-baseline table still reported eight crates, an absent lint engine, and
   a review date predating completed `0.1` work.
4. **P1 queued as DS-CI-003:** permanent workflows contain mutable third-party Action references.
   Pinning, Node-runtime review, and automated policy enforcement require one repository-wide change.
5. **P2 queued as DS-QUALITY-001:** advisory, unused-dependency, fuzzing, property, benchmark, and
   portability checks should become durable CI rather than one-time audit probes.

The temporary audit workflows and result files are not product infrastructure and are removed by the
final audit commit.

## Known Architectural Debt
"""
replace_once("docs/development/dartscope-library-plan.md", behaviors_anchor, audit_section)

release_sentence_old = """3. Added an executable nine-crate publish order with metadata/topology validation and generated
   `.crate` archive inspection.
"""
release_sentence_new = """3. Added an ordered nine-crate publish topology with metadata validation and generated `.crate`
   archive inspection; DS-AUDIT-001 corrected the shell-script execution boundary found later.
"""
replace_once(
    "docs/development/dartscope-library-plan.md",
    release_sentence_old,
    release_sentence_new,
)

compat_anchor = """### DS-COMPAT-001: Upstream Compatibility Radar

Status: research. Priority: P3. Not on the current 0.1 critical path.
"""
roadmap_insert = """### DS-AUDIT-001: Full Repository Audit Corrections

Status: verified. Priority: P0. Prerequisite: DS-RELEASE-001.

Implemented (2026-07-17):

1. Re-ran exact Rust 1.95 checks, all-feature tests, Clippy, rustdoc, isolated umbrella features,
   standard Ubuntu/Windows CI, and all nine package archives.
2. Added a permanent repository-consistency checker for workspace/release topology, roadmap state,
   changelog/tag truthfulness, and publish-script execution.
3. Corrected the non-executable publish script boundary by using an explicit Bash invocation and
   retaining executable Git mode.
4. Returned unreleased `0.1.0` notes to `Unreleased` until tag `v0.1.0` actually exists.
5. Corrected the baseline crate count, lint status, review date, and release evidence.
6. Removed every temporary audit workflow, trigger, and result file after recording reusable findings.

Acceptance:

- a protected manual publish cannot fail merely because the script executable bit was lost;
- the changelog never claims a release whose exact version tag is absent;
- stale baseline claims fail the permanent consistency check;
- the final standard Ubuntu/Windows matrix and release package checker pass.

### DS-CI-003: Immutable Actions And CI Supply Chain

Status: ready. Priority: P0. Prerequisite: DS-AUDIT-001.

Required work:

1. Inventory every permanent `uses:` reference, including list-form `- uses:` entries.
2. Pin third-party Actions to reviewed immutable commit SHAs and retain the human-readable release
   tag in an adjacent comment.
3. Replace Action majors that still depend on a deprecated Node runtime with supported releases.
4. Add `actionlint` plus a repository policy check that rejects mutable refs, unknown workflow
   permissions, and unreviewed `pull_request_target` or write-token changes.
5. Preserve minimal permissions and protected-environment boundaries for crates.io publication.
6. Record and classify hosted-runner flakes; one clean retry may clear infrastructure failures, but
   recurring platform failures must become blocking fixtures or issues.

Acceptance:

- every permanent third-party Action is SHA-pinned;
- workflow syntax and policy checks run on Linux before other expensive jobs;
- ordinary pull requests cannot obtain release credentials or write permissions;
- the aggregate status distinguishes product failures from a documented runner retry.

### DS-CLI-003: Lint Command, Configuration, And SARIF

Status: ready. Priority: P1. Prerequisite: DS-LINT-001.

Required work:

1. Add `dartscope lint <project>` without moving rule semantics into the CLI crate.
2. Define a documented TOML configuration for rule enablement, severity overrides, import patterns,
   layer boundaries, naming options, and orphan entry points.
3. Register a new command-facing JSON envelope with its own schema ID, compatibility tests, and
   checked-in golden fixtures.
4. Add SARIF 2.1 output with rule metadata, normalized paths, exact spans, severity, and related-path
   evidence suitable for GitHub Code Scanning.
5. Define stable exit codes for clean analysis, findings at the configured failure threshold,
   invalid configuration, malformed project input, and filesystem errors.
6. Cover Linux and Windows process behavior, paths with spaces, malformed configuration, deterministic
   output, stdout/stderr separation, and `--deny-warnings` behavior.

Acceptance:

- every finding maps back to the existing source-free lint engine;
- JSON and SARIF order is deterministic and versioned independently;
- default configuration remains inert unless the caller enables rules;
- a documented GitHub Actions example can upload SARIF without custom parsing.

### DS-INDEX-005: Incremental Workspace Index

Status: ready. Priority: P1. Prerequisites: DS-INDEX-004, DS-AUDIT-001.

Required work:

1. Add a stateful index with explicit `upsert_file`, `remove_file`, configuration update, and immutable
   snapshot operations over normalized inputs.
2. Maintain reverse import/export/part edges and invalidate only affected libraries, namespaces,
   GraphQL bindings, references, and lint contexts.
3. Preserve deterministic output equivalence with a clean full rebuild after every update sequence.
4. Define thread-safety and snapshot ownership without exposing parser ASTs or performing hidden I/O.
5. Add operation counters and benchmarks for 1k- and 10k-file synthetic workspaces.

Acceptance:

- incremental and full rebuild snapshots compare equal in property tests;
- removing or changing a library invalidates every dependent result and no unrelated result;
- memory and update-time baselines are checked without making wall-clock flakes blocking;
- existing stateless APIs remain available.

### DS-INDEX-006: Broader Reference And Scope Resolution

Status: planned. Priority: P1. Prerequisites: DS-INDEX-005, DS-PARSE-006.

Required work:

1. Add conservative references for type positions, constructors, variable reads/writes, assignments,
   annotations, and supported patterns with exact spans and confidence.
2. Model lexical scopes and shadowing before treating unqualified identifier tokens as semantic
   references.
3. Add constructor/member and supported extension lookup without claiming analyzer-equivalent type
   inference or overload resolution.
4. Retain missing, ambiguous, non-visible, and external-unindexed candidates rather than guessing.
5. Add deterministic find-definition and find-references batch APIs that reuse one workspace context.

Acceptance:

- declaration/reference fixtures include nearby shadowing and false-positive negatives;
- every new reference kind is opt-in until its compatibility contract is documented;
- existing invocation-target reference output is unchanged;
- index code never reparses raw source.

### DS-LSP-001: Language Server Foundation

Status: planned. Priority: P2. Prerequisites: DS-INDEX-005, DS-INDEX-006, DS-CLI-003.

Required work:

1. Add an optional `dartscope-lsp` crate with standard input/output transport isolated from analysis
   crates.
2. Implement lifecycle, incremental document synchronization, diagnostics, document symbols,
   workspace symbols, definition, references, and evidence-based hover.
3. Surface parser capability limits and stale-snapshot states explicitly.
4. Integrate lint diagnostics and navigation without inventing member/type results unavailable from
   the index.
5. Add protocol fixtures, cancellation tests, deterministic diagnostics, and editor smoke tests.

Acceptance:

- the server remains responsive under cancellation and rapid file replacement;
- all positions round-trip correctly for LF, CRLF, and UTF-16 LSP coordinates;
- no filesystem scan or SDK process is hidden inside core/index APIs;
- unsupported requests return honest empty/partial capability responses.

### DS-QUALITY-001: Durable Security, Fuzzing, And Performance Gates

Status: ready. Priority: P1. Prerequisite: DS-AUDIT-001.

Required work:

1. Add pinned RustSec advisory and unused-dependency checks with an explicit allowlist policy and
   reviewed expiration dates.
2. Add fuzz targets for lexical masking, directives, pubspec/package-config parsing, GraphQL
   extraction, and URI normalization.
3. Add property tests for span monotonicity, deterministic ordering, incremental/full equivalence,
   combinator visibility, and panic-free malformed input.
4. Add non-flaky benchmark baselines for parsing, project indexing, reference resolution, and package
   archive generation.
5. Add macOS as a non-blocking portability signal for the `0.2` cycle and define promotion criteria.

Acceptance:

- malformed inputs never panic in the bounded fuzz corpus;
- advisory and unused-dependency findings cannot be silently ignored;
- benchmarks report regressions without relying on unstable absolute hosted-runner timings;
- every exception has an owner, rationale, and review date.

### DS-PARSE-007: Alternative Parser Backend Evaluation

Status: research. Priority: P2. Prerequisites: DS-INDEX-006, DS-QUALITY-001.

Research scope:

1. Compare a tree-sitter backend and an out-of-process official analyzer bridge against the existing
   `DartParser` capability contract.
2. Evaluate Dart syntax/version coverage, recovery, span fidelity, incremental updates, license,
   maintenance, binary size, process isolation, and Rust 1.95 compatibility.
3. Prototype only a bounded normalized-fact exchange; do not expose backend AST types publicly.
4. Define hybrid selection and fallback behavior without silently mixing confidence levels.

Research exit:

- one backend is selected for an implementation task or both are rejected with evidence;
- normalized fixture parity and capability differences are documented;
- security/process boundaries and packaging impact are explicit.

### DS-COMPAT-001: Upstream Compatibility Radar

Status: research. Priority: P2. Not on the current 0.1 critical path.
"""
replace_once("docs/development/dartscope-library-plan.md", compat_anchor, roadmap_insert)

changelog = """# Changelog

All notable changes to DartScope are documented in this file.

The format follows Keep a Changelog, and the project uses semantic versioning while remaining
pre-1.0.

## [Unreleased]

### Added

- Nine publishable Rust crates covering normalized Dart analysis, parsing, package and URI
  resolution, project indexing, optional lint rules, Flutter conventions, versioned JSON contracts,
  a thin umbrella API, and the `dartscope` CLI.
- Conservative source-only Dart and Flutter analysis with exact spans, diagnostics, capability
  metadata, namespace and reference resolution, GraphQL contract linking, and package-aware Flutter
  catalogs.
- Stable v1 CLI JSON envelopes, deterministic fixtures, explicit exit codes, and Linux/Windows
  process-level coverage.
- Versioned opt-in ecosystem conventions for `go_router`, Provider, Riverpod, and BLoC.
- Release metadata, package-order validation, package archives, support documentation, and a
  manually gated crates.io publishing workflow.

### Compatibility

- Minimum supported Rust version: 1.95.
- Workspace edition: Rust 2024 with resolver 3.
- Dart and Flutter support is capability-based and source-only; DartScope does not execute SDK
  tools during normal analysis.
- Existing command-facing JSON contracts remain at schema version v1.

Release notes remain under `Unreleased` until the exact version tag exists. The release process moves
this content to a dated version section and adds compare/release links in the same release operation.

[Unreleased]: https://github.com/RusTokRs/dartscope/commits/main
"""
(ROOT / "CHANGELOG.md").write_text(changelog, encoding="utf-8")

release_readiness_old = """The workspace version is inherited from the root `Cargo.toml`. Before tagging a release, verify that
the version and changelog agree, then run:
"""
release_readiness_new = """The workspace version is inherited from the root `Cargo.toml`. Before an exact version tag exists,
all notes must remain under `Unreleased`; do not add a dated version heading or release link early.
During the release operation, move those notes to `## [<version>] - YYYY-MM-DD`, add compare/release
links, create the matching `v<version>` tag, and verify that all four values agree. Then run:
"""
replace_once("docs/release-process.md", release_readiness_old, release_readiness_new)

release_commands_old = """cargo test -p dartscope --all-features --locked
python3 tools/check-release-packages.py
"""
release_commands_new = """cargo test -p dartscope --all-features --locked
python3 tools/check-repository-consistency.py
python3 tools/check-release-packages.py
bash -n tools/publish-crates.sh
"""
replace_once("docs/release-process.md", release_commands_old, release_commands_new)

release_workflow_path = ROOT / ".github/workflows/release.yml"
release_workflow = release_workflow_path.read_text(encoding="utf-8")
release_workflow = release_workflow.replace(
    "      - name: Validate release packages\n        run: python3 tools/check-release-packages.py\n",
    "      - name: Validate repository consistency\n        env:\n          DARTSCOPE_VERIFY_RELEASE_TAG: 1\n        run: python3 tools/check-repository-consistency.py\n      - name: Validate release packages\n        run: python3 tools/check-release-packages.py\n",
)
release_workflow = release_workflow.replace(
    "        run: tools/publish-crates.sh\n",
    "        run: bash tools/publish-crates.sh\n",
)
if "bash tools/publish-crates.sh" not in release_workflow:
    raise SystemExit("release workflow publish invocation was not updated")
release_workflow_path.write_text(release_workflow, encoding="utf-8")

ci_path = ROOT / ".github/workflows/ci.yml"
ci = ci_path.read_text(encoding="utf-8")
ci = ci.replace(
    "      - name: Check formatting\n        run: cargo fmt --all -- --check\n",
    "      - name: Check formatting\n        run: cargo fmt --all -- --check\n      - name: Check repository consistency\n        run: python3 tools/check-repository-consistency.py\n",
)
ci = ci.replace(
    "actions/github-script@v7",
    "actions/github-script@f28e40c7f34bde8b3046d885e986cb6290c5673b # v7",
)
if ci.count("actions/github-script@f28e40c7f34bde8b3046d885e986cb6290c5673b") != 4:
    raise SystemExit("expected four github-script references")
ci_path.write_text(ci, encoding="utf-8")

consistency = r'''#!/usr/bin/env python3
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
'''
(ROOT / "tools/check-repository-consistency.py").write_text(consistency, encoding="utf-8")
