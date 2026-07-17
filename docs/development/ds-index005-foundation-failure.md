---
id: doc://docs/development/ds-index005-foundation-failure.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Foundation Failure

Failed stage: `index tests (exit 101)`.

The implementation was not committed. Temporary staging files were removed.

```text
===== decode implementation =====
===== apply implementation =====
DS-INDEX-005 stateful foundation applied
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
Ran 6 tests in 0.008s

OK
===== workflow policy =====
workflow policy passed
===== install Rust 1.95 =====
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: downloading 5 components

  1.95.0-x86_64-unknown-linux-gnu installed - rustc 1.95.0 (59807616e 2026-04-14)

===== rustfmt =====
===== index tests =====
error[E0277]: can't compare `&DartDiagnostic` with `DartDiagnostic`
   --> crates/dartscope-index/src/incremental.rs:647:72
    |
647 |             .find(|(index, candidate)| !consumed[*index] && *candidate == diagnostic)
    |                                                                        ^^ no implementation for `&DartDiagnostic == DartDiagnostic`
    |
    = help: the trait `PartialEq<DartDiagnostic>` is not implemented for `&DartDiagnostic`
    = note: required for `&&DartDiagnostic` to implement `PartialEq<&DartDiagnostic>`
help: consider dereferencing here
    |
647 |             .find(|(index, candidate)| !consumed[*index] && **candidate == diagnostic)
    |                                                             +

For more information about this error, try `rustc --explain E0277`.
error: could not compile `dartscope-index` (lib) due to 1 previous error
```
