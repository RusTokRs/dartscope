---
id: doc://docs/development/ds-index005-semantic-fix-failure.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Semantic Invalidation Failure

Failed stage: `index tests (exit 101)`.

The correction was not committed. Temporary staging files were removed.

```text
===== decode correction =====
===== apply correction =====
DS-INDEX-005 symbol-name invalidation correction applied
===== install actionlint =====
go: downloading github.com/rhysd/actionlint v1.7.12
go: github.com/rhysd/actionlint@v1.7.12 requires go >= 1.25.0; switching to go1.25.12
go: downloading go1.25.12 (linux/amd64)
go: downloading github.com/bmatcuk/doublestar/v4 v4.10.0
go: downloading github.com/fatih/color v1.19.0
go: downloading github.com/mattn/go-colorable v0.1.14
go: downloading github.com/mattn/go-runewidth v0.0.21
go: downloading github.com/mattn/go-shellwords v1.0.12
go: downloading github.com/robfig/cron/v3 v3.0.1
go: downloading go.yaml.in/yaml/v4 v4.0.0-rc.3
go: downloading golang.org/x/sync v0.20.0
go: downloading golang.org/x/sys v0.42.0
go: downloading github.com/mattn/go-isatty v0.0.20
go: downloading github.com/clipperhouse/uax29/v2 v2.7.0
===== actionlint =====
===== workflow policy tests =====
test_accepts_reviewed_inventory (test_workflow_policy.WorkflowPolicyTests.test_accepts_reviewed_inventory) ... ok
test_rejects_missing_release_comment (test_workflow_policy.WorkflowPolicyTests.test_rejects_missing_release_comment) ... ok
test_rejects_mutable_list_form_action (test_workflow_policy.WorkflowPolicyTests.test_rejects_mutable_list_form_action) ... ok
test_rejects_unknown_permission_and_pull_request_target (test_workflow_policy.WorkflowPolicyTests.test_rejects_unknown_permission_and_pull_request_target) ... ok
test_rejects_unreviewed_write_permission (test_workflow_policy.WorkflowPolicyTests.test_rejects_unreviewed_write_permission) ... ok
test_requires_push_only_status_reporter (test_workflow_policy.WorkflowPolicyTests.test_requires_push_only_status_reporter) ... ok

----------------------------------------------------------------------
Ran 6 tests in 0.009s

OK
===== workflow policy =====
workflow policy passed
===== install Rust 1.95 =====
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: downloading 5 components

  1.95.0-x86_64-unknown-linux-gnu installed - rustc 1.95.0 (59807616e 2026-04-14)

===== rustfmt =====
===== same-name evidence regression =====

running 1 test
.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 45 filtered out; finished in 0.00s


running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

===== part sibling regression =====

running 1 test
.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 45 filtered out; finished in 0.00s


running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

===== index tests =====

running 46 tests
.................... 20/46
tests::incremental::pubspec_updates_refresh_package_resolution_without_reparsing_files --- FAILED
.........................
failures:

---- tests::incremental::pubspec_updates_refresh_package_resolution_without_reparsing_files stdout ----

thread 'tests::incremental::pubspec_updates_refresh_package_resolution_without_reparsing_files' (4396) panicked at crates/dartscope-index/src/tests/incremental.rs:201:5:
assertion `left == right` failed
  left: 3
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::incremental::pubspec_updates_refresh_package_resolution_without_reparsing_files

test result: FAILED. 45 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p dartscope-index --lib`
```
