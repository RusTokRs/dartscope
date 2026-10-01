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

Temporary verification workflows that produce source files before a failing gate must not rely on
`git reset --hard` alone: Git leaves untracked generated files in the worktree. Failure handlers must
run `bash tools/reset-verification-worktree.sh`, then selectively remove tracked staging files and write
the failure report. The helper performs fetch, hard reset, and `git clean -fdx`; repository consistency
guards all three commands. This rule was added after the first DS-INDEX-005 failure accidentally
retained untracked foundation files even though its report claimed that no implementation was committed.

## Branch Protection

Nothing in the repository makes the checks of `ci.yml` a condition of a merge. On 2026-10-02 `main` has no
protection rule and the repository has no ruleset (`gh api repos/RusTokRs/dartscope/rulesets` answers `[]`).
The token that did the audit work cannot change repository settings (the settings endpoints answer HTTP 403
for it), so enabling the rule is an action for a repository administrator.

The recommended ruleset for the default branch: require a pull request, forbid force pushes and deletion, and
require the checks below. The names are the job names as GitHub shows them. The aggregate status
`dartscope/ci` is published only for pushes, not for pull requests, so it cannot be the required check there.
A check name is selectable in the settings only after the job has run once on a pull request.

| Job in `ci.yml` | Required check |
| --- | --- |
| `workflow_policy` | `Workflow policy` |
| `dependency_quality` | `Dependency security and hygiene` |
| `quality` | `Quality gates` |
| `test` | `Tests (ubuntu-latest)`, `Tests (windows-latest)` |
| `macos_portability` | `macOS 15 arm64 portability` |
| `benchmark_report` | `Benchmark regression report` |
| `fuzz` | `Bounded fuzz corpus` |
| `edition_2024` | `Edition 2024 (<os> / <check>)` for `ubuntu-latest` and `windows-latest` with `workspace-all-targets`, `umbrella-minimal`, `umbrella-all-features` |

The same ruleset as a request (an administrator runs it once; `~DEFAULT_BRANCH` follows a rename of the default
branch):

```sh
gh api repos/RusTokRs/dartscope/rulesets --method POST --input - <<'JSON'
{
  "name": "main requires CI",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": {
      "include": [
        "~DEFAULT_BRANCH"
      ],
      "exclude": []
    }
  },
  "rules": [
    {
      "type": "deletion"
    },
    {
      "type": "non_fast_forward"
    },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "required_status_checks": [
          {
            "context": "Workflow policy"
          },
          {
            "context": "Dependency security and hygiene"
          },
          {
            "context": "Quality gates"
          },
          {
            "context": "Tests (ubuntu-latest)"
          },
          {
            "context": "Tests (windows-latest)"
          },
          {
            "context": "macOS 15 arm64 portability"
          },
          {
            "context": "Benchmark regression report"
          },
          {
            "context": "Bounded fuzz corpus"
          },
          {
            "context": "Edition 2024 (ubuntu-latest / workspace-all-targets)"
          },
          {
            "context": "Edition 2024 (ubuntu-latest / umbrella-minimal)"
          },
          {
            "context": "Edition 2024 (ubuntu-latest / umbrella-all-features)"
          },
          {
            "context": "Edition 2024 (windows-latest / workspace-all-targets)"
          },
          {
            "context": "Edition 2024 (windows-latest / umbrella-minimal)"
          },
          {
            "context": "Edition 2024 (windows-latest / umbrella-all-features)"
          }
        ]
      }
    }
  ]
}
JSON
```

Requiring zero approving reviews keeps a single maintainer able to merge a green pull request; raise
`required_approving_review_count` when there is a second reviewer. Adding or renaming a job in `ci.yml` means
changing this list in the same change.

## Maintenance Limits

Action release reviews and SHA updates are currently manual. Mutable major tags and automated
unreviewed upgrades remain forbidden. New workflow files, permissions, events, self-hosted runners, or
Actions must update the policy, tests, inventory table, and roadmap together.
