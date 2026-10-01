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

## Security-Relevant Dependencies

`dartscope-resolve` decides which paths a project may name: `package:` URIs and the `rootUri` and `packageUri`
values of `package_config.json`. It used to delegate the URI syntax and the reference resolution to
`uriparse 0.6.4`, whose last release on crates.io is dated 2022-03-18, so an advisory filed against it would
probably not have been fixed upstream. The 2026-10 audit replaced it with a module of the crate itself
(`crates/dartscope-resolve/src/uri.rs`: the syntax check of RFC 3986, the reference resolution of section 5.2
with a linear dot-segment removal, and printing) and the crate has no URI dependency any more. A comparison of
the two on 400,000 damaged URIs, run before `uriparse` was removed, found that the dependency was the weaker
side: it panics in `resolve` when the result would start with `//` without an authority, rejects absolute paths
with a colon in the first segment (`/C:/x`) and ports above 65535, and rewrites what it prints. The module is
checked by the examples of RFC 3986 section 5.4, by accept and reject lists, by the regression cases from that
comparison, by the fixtures for roots that leave the project, `..` segments, percent-encoded roots and escaped
separators in `dartscope-resolve`, and by the `uri_normalization` fuzz target.

The rule that follows for a dependency that takes part in a path decision: look at its maintenance state when it
is added, and when it is stale and the part that is used is small, own that part and test it against the
specification instead of adding an exception to the expiring-exception policy above. The `cargo-audit` job stays
the detector for the dependencies that remain (`serde_json` for the package configuration, `percent-encoding`
for decoding, the YAML backend for `pubspec.yaml`).

Tool versions are duplicated deliberately in CI and the policy file; `check-dependency-policy.py` rejects
pin drift. Updating either tool requires reviewing its release, Rust 1.95 compatibility, output behavior,
and the complete exception list. Network or registry bootstrap failures are infrastructure failures and
must not be converted into dependency allowlist entries.
