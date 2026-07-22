---
id: doc://docs/development/macos-portability.md
kind: development_policy
language: en
source_language: en
status: active
---

# macOS Portability Gate

DartScope runs a blocking macOS portability gate in permanent CI. The gate detects Apple-platform
build, test, filesystem, archive, and arm64 assumptions and fails the CI workflow when the repository
no longer satisfies its macOS release contract.

## Current Gate

The job uses the versioned `macos-15` GitHub-hosted runner label rather than the moving
`macos-latest` alias. It records `sw_vers`, `uname`, and the exact Rust compiler identity in the step
summary, then runs:

1. `cargo +1.95.0 check --workspace --all-targets --locked`;
2. `cargo +1.95.0 test --workspace --locked --quiet`;
3. `cargo +1.95.0 package --workspace --locked --allow-dirty --no-verify`, followed by an exact
   nine-archive count.

The job has a 30-minute timeout, does not use `continue-on-error`, and is included in the aggregate
`dartscope/ci` status together with the benchmark regression gate. A macOS failure therefore blocks
merge and default-branch release readiness.

## Historical Observation

The pinned `macos-15` signal was first validated in pull-request CI run `29698405538` on July 19,
2026. Environment capture, all-target workspace checking, the full workspace test suite, and the exact
nine-archive package contract passed. The former non-blocking observation phase was retired by the
reviewed DS-BUG-005 remediation because declared portability regressions must not be masked by a green
pipeline.

## Failure Triage

Every failure must be classified before changing the gate:

- **product defect:** reproducible source, test, path, archive, target, or dependency behavior;
- **runner-image drift:** an image or preinstalled-tool change outside the repository;
- **service instability:** provisioning, networking, registry, or hosted-runner outage;
- **unsupported assumption:** a documented platform limitation that requires an explicit product
  decision rather than a retry or suppression.

Infrastructure failures may be retried, but they must not be converted into code exceptions or weaker
assertions. Product defects remain open until fixed or explicitly rejected with rationale.

## Blocking Policy

The gate remains blocking unless a reviewed policy change documents a narrower supported platform
contract. Platform-specific ignores, dependency exceptions, reduced test sets, archive bypasses, and
job-level `continue-on-error` are not acceptable substitutes for fixing a reproducible defect.

A runner-image migration requires a reviewed compatibility report showing that the replacement image
provides equivalent or stronger evidence. Queue time and absolute wall-clock duration are not reasons
to weaken the gate.
