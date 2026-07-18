---
id: doc://docs/development/ds-quality001-policy-test-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-QUALITY-001 Policy Test Diagnostic

- apply exit: `0`
- unittest exit: `1`

```text
test_dependency_policy (unittest.loader._FailedTest.test_dependency_policy) ... ERROR
test_accepts_reviewed_inventory (test_workflow_policy.WorkflowPolicyTests.test_accepts_reviewed_inventory) ... ok
test_rejects_missing_release_comment (test_workflow_policy.WorkflowPolicyTests.test_rejects_missing_release_comment) ... ok
test_rejects_mutable_list_form_action (test_workflow_policy.WorkflowPolicyTests.test_rejects_mutable_list_form_action) ... ok
test_rejects_unknown_permission_and_pull_request_target (test_workflow_policy.WorkflowPolicyTests.test_rejects_unknown_permission_and_pull_request_target) ... ok
test_rejects_unreviewed_write_permission (test_workflow_policy.WorkflowPolicyTests.test_rejects_unreviewed_write_permission) ... ok
test_requires_push_only_status_reporter (test_workflow_policy.WorkflowPolicyTests.test_requires_push_only_status_reporter) ... ok

======================================================================
ERROR: test_dependency_policy (unittest.loader._FailedTest.test_dependency_policy)
----------------------------------------------------------------------
ImportError: Failed to import test module: test_dependency_policy
Traceback (most recent call last):
  File "/usr/lib/python3.12/unittest/loader.py", line 394, in _find_test_path
    module = self._get_module_from_name(name)
             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.12/unittest/loader.py", line 337, in _get_module_from_name
    __import__(name)
  File "/home/runner/work/dartscope/dartscope/tools/tests/test_dependency_policy.py", line 32
    f"[advisories]
    ^
SyntaxError: unterminated f-string literal (detected at line 32)


----------------------------------------------------------------------
Ran 7 tests in 0.009s

FAILED (errors=1)
```
