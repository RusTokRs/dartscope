---
id: doc://docs/development/ci-supply-chain.md
kind: development_policy
language: en
source_language: en
status: active
---

# CI Supply-Chain Policy

This document records the reviewed GitHub Actions and token boundaries for the DartScope `0.2`
development cycle. The crate manifests remain on the unreleased `0.1.0` package version until the
release process creates the exact `v0.1.0` tag; roadmap work can proceed independently of that tag.

## Reviewed Action Inventory

Every permanent `uses:` reference is checked against `tools/check-workflow-policy.py`. New Actions or
version changes require an explicit policy and documentation update in the same change.

| Action | Reviewed release | Immutable commit | Runtime | Purpose |
| --- | --- | --- | --- | --- |
| `actions/checkout` | `v6.0.2` | `de0fac2e4500dabe0009e67214ff5f5447ce83dd` | Node 24 | Read-only source checkout |
| `actions/github-script` | `v9.0.0` | `3a2844b7e9c422d3c10d287c895573f7108da1b3` | Node 24 | Push/workflow-dispatch aggregate commit status |
| `actions/upload-artifact` | `v7.0.1` | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | Node 24 | Release-package archive upload |

`dtolnay/rust-toolchain@master` was removed. CI and release jobs install the repository-pinned Rust
`1.95.0` toolchain directly through `rustup`, eliminating a mutable Action dependency.

The Node 24 Action releases require Actions Runner `2.327.1` or newer. DartScope's blocking workflows
use GitHub-hosted `ubuntu-latest` and `windows-latest` runners. A future self-hosted runner is
unsupported until its version is checked and registered in this policy.

## Workflow Linting And Policy

Both permanent workflows install `actionlint v1.7.12` from its exact Go module version before Rust
compilation or packaging. Go's module checksum verification protects the downloaded module, while the
hosted runner still owns the Go toolchain and module-proxy availability. If this bootstrap becomes
unreliable, DS-QUALITY-001 must replace it with a checksum-pinned binary or a reviewed immutable Action.

The repository policy rejects:

- mutable, unknown, or incorrectly documented `uses:` references, including list-form entries;
- workflow files not registered in the permanent inventory;
- missing, aggregate, unknown, or invalid workflow permissions;
- write permissions outside the explicit per-workflow allowlist;
- `pull_request_target` without a reviewed policy change;
- release publishing without `workflow_dispatch`, an exact version tag, the protected `crates-io`
  environment, and step-scoped registry credentials.

Pull request jobs are read-only. The only GitHub write permission is `statuses: write` on the
push/workflow-dispatch aggregate reporter; that job is skipped for `pull_request` events. Checkout
credentials are not persisted.

## Failure And Retry Classification

A deterministic policy, format, Clippy, rustdoc, test, or package failure is a product failure and must
be fixed rather than retried. A hosted-runner provisioning, network, or GitHub service failure may
receive one clean retry. The aggregate `dartscope/ci` description records `github.run_attempt`, so a
successful retry remains visible. A repeated failure on the same platform becomes a blocking fixture
or tracked issue before the roadmap item can stay verified.

The repository audit observed one transient Windows failure that did not reproduce on the clean audit
head. It is classified as an infrastructure flake, not evidence that Windows coverage can be removed.

## Maintenance Limits

Action release reviews and SHA updates are currently manual. Mutable major tags and automated
unreviewed upgrades remain forbidden. New workflow files, permissions, events, self-hosted runners, or
Actions must update the policy, tests, inventory table, and roadmap together.
