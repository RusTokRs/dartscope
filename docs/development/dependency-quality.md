---
id: doc://docs/development/dependency-quality.md
kind: development_contract
language: en
source_language: en
status: active
---

# Dependency Security And Hygiene

Permanent CI installs exact `cargo-audit 0.22.2` and `cargo-machete 0.9.2` releases with Cargo's
`--locked` installation mode. The dependency job runs on pushes, pull requests, manual dispatches, and a
weekly schedule. It is read-only and participates in the aggregate `dartscope/ci` result.

## Exception Policy

`tools/dependency-exceptions.toml` is the review source of truth. Every RustSec advisory or unused-
dependency exception must include:

- the exact advisory ID or manifest/dependency pair;
- a non-empty owner;
- a concrete rationale of at least 20 characters;
- an ISO expiration date that has not passed.

RustSec IDs must match `.cargo/audit.toml` exactly. Unused-dependency exceptions must match native
`package.metadata.cargo-machete.ignored` or `workspace.metadata.cargo-machete.ignored` entries exactly.
The checker rejects either an undocumented native ignore or a policy entry not applied to its tool.
Empty exception lists are the preferred baseline.

`cargo-audit` denies known vulnerabilities, yanked dependencies, and configured unmaintained warnings.
`cargo-machete` is intentionally run without `--with-metadata`: its static scan cannot mutate `Cargo.lock`
and any false positive must pass through the same expiring review policy rather than being silently
suppressed.

## Maintenance Boundary

The initial unused-dependency scan found a real direct `serde` declaration in `dartscope-parse` with no
crate-local use. It and the stale package-level lock edge were removed instead of allowlisted; exceptions
are reserved for reviewed false positives. Generated policy code is tested with Python syntax warnings
promoted to errors so regex escapes cannot regress silently.

Tool versions are duplicated deliberately in CI and the policy file; `check-dependency-policy.py` rejects
pin drift. Updating either tool requires reviewing its release, Rust 1.95 compatibility, output behavior,
and the complete exception list. Network or registry bootstrap failures are infrastructure failures and
must not be converted into dependency allowlist entries.
