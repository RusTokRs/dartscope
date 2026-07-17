---
id: doc://docs/development/ds-index005-symbol-fix-failure.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Symbol Invalidation Failure

Failed stage: `focused symbol regression (exit 101)`.

The correction was not committed. Temporary staging files were removed.

```text
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
go: downloading github.com/clipperhouse/uax29/v2 v2.7.0
go: downloading github.com/mattn/go-isatty v0.0.20
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
===== focused symbol regression =====
error[E0716]: temporary value dropped while borrowed
   --> crates/dartscope-index/src/tests/incremental.rs:355:20
    |
355 |       let initial = &index
    |  ____________________^
356 | |         .snapshot()
    | |___________________^ creates a temporary value which is freed while still in use
357 |           .identifier_reference_resolutions()
358 |           .resolutions[0];
    |                          - temporary value is freed at the end of this statement
359 |       assert_eq!(initial.status, DartSymbolResolutionStatus::NotVisible);
    |       ------------------------------------------------------------------ borrow later used here
    |
help: consider using a `let` binding to create a longer lived value
    |
355 ~     let binding = index
356 +         .snapshot();
357 ~     let initial = &binding
    |

error[E0716]: temporary value dropped while borrowed
   --> crates/dartscope-index/src/tests/incremental.rs:395:23
    |
395 |       let resolution = &index
    |  _______________________^
396 | |         .snapshot()
    | |___________________^ creates a temporary value which is freed while still in use
397 |           .identifier_reference_resolutions()
398 |           .resolutions[0];
    |                          - temporary value is freed at the end of this statement
399 |       assert_eq!(resolution.status, DartSymbolResolutionStatus::Missing);
    |       ------------------------------------------------------------------ borrow later used here
    |
help: consider using a `let` binding to create a longer lived value
    |
395 ~     let binding = index
396 +         .snapshot();
397 ~     let resolution = &binding
    |

For more information about this error, try `rustc --explain E0716`.
error: could not compile `dartscope-index` (lib test) due to 2 previous errors
```
