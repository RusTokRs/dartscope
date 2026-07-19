---
id: doc://docs/development/macos-portability.md
kind: development_policy
language: en
source_language: en
status: active
---

# macOS Portability Signal

DartScope runs a non-blocking macOS portability job in permanent CI for the `0.2` observation cycle.
The signal detects Apple-platform build, test, filesystem, archive, and arm64 assumptions without
changing the Linux/Windows release gate.

## Current Signal

The job uses the versioned `macos-15` GitHub-hosted runner label rather than the moving
`macos-latest` alias. It records `sw_vers`, `uname`, and the exact Rust compiler identity in the step
summary, then runs:

1. `cargo +1.95.0 check --workspace --all-targets --locked`;
2. `cargo +1.95.0 test --workspace --locked --quiet`;
3. `cargo +1.95.0 package --workspace --locked --allow-dirty --no-verify`, followed by an exact
   nine-archive count.

The job has `continue-on-error: true`, has a 30-minute timeout, and is deliberately absent from the
`report.needs` list that produces the aggregate `dartscope/ci` status. A macOS failure is therefore
visible and actionable but cannot hide Linux/Windows release readiness or block unrelated changes.

## Observation Record

The observation window starts with the first successful default-branch execution of the exact signal
above. A run is valid only when the job reaches all three repository checks; cancelled, provisioning,
network, and registry failures are classified separately and do not count toward promotion.

### Initial implementation validation

Pull-request CI run `29698405538` completed the pinned `macos-15` job successfully on July 19, 2026.
Environment capture, all-target workspace checking, the full workspace test suite, and the exact
nine-archive package contract all passed. This validates the signal implementation but does not count
as a default-branch observation for promotion.

## Failure Triage

Every failure must be classified before changing the signal:

- **product defect:** reproducible source, test, path, archive, target, or dependency behavior;
- **runner-image drift:** an image or preinstalled-tool change outside the repository;
- **service instability:** provisioning, networking, registry, or hosted-runner outage;
- **unsupported assumption:** a documented platform limitation that requires an explicit product
  decision rather than a retry or suppression.

Infrastructure failures may be retried, but they must not be converted into code exceptions or
weaker assertions. Product defects remain open until fixed or explicitly rejected with rationale.

## Promotion Criteria

The macOS signal may become part of the blocking `dartscope/ci` gate only through a reviewed policy
change and only when all of the following are true:

1. the pinned runner label is a generally available image and is not under active deprecation;
2. at least 30 valid default-branch or scheduled observations span at least six weeks;
3. at least 95% of valid observations pass, excluding documented service-instability runs;
4. there is no unresolved reproducible macOS-only product defect;
5. no platform-specific ignore, dependency exception, reduced test set, or archive bypass is needed;
6. the same check, workspace-test, and nine-archive contract can run with `continue-on-error` removed;
7. the aggregate-status policy and roadmap are updated in the same reviewed change.

A runner-image migration restarts the observation window unless a reviewed compatibility report
shows that the old and new images provide equivalent evidence. Queue time and absolute wall-clock
duration are not promotion criteria.
