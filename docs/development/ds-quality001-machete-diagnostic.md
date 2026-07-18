---
id: doc://docs/development/ds-quality001-machete-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-QUALITY-001 Cargo Machete Diagnostic

- apply exit: `0`
- Rust install exit: `0`
- cargo-machete install exit: `0`
- machete exit: `1`

```text
Analyzing dependencies of crates in this directory...
cargo-machete found the following unused dependencies in this directory:
dartscope-parse -- ./crates/dartscope-parse/Cargo.toml:
	serde

If you believe cargo-machete has detected an unused dependency incorrectly,
you can add the dependency to the list of dependencies to ignore in the
`[package.metadata.cargo-machete]` section of the appropriate Cargo.toml.
For example:

[package.metadata.cargo-machete]
ignored = ["prost"]

You can also try running it with the `--with-metadata` flag for better accuracy,
though this may modify your Cargo.lock files.

Done!
```
