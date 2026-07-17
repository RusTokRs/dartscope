---
id: doc://docs/release-process.md
kind: release_policy
language: en
source_language: en
status: active
---

# DartScope Release Process

## Release Readiness

The workspace version is inherited from the root `Cargo.toml`. Before tagging a release, verify that
the version and changelog agree, then run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
cargo test -p dartscope --all-features --locked
python3 tools/check-release-packages.py
```

The package checker validates required metadata, versioned internal dependencies, publish-order
topology, and the generated `.crate` archives. Packaging uses `--no-verify` because the first release
cannot resolve not-yet-published sibling crates from crates.io; the workspace tests and rustdoc gate
provide the build verification before publication.

## Crate Publication Order

`tools/release-crates.txt` is the executable source of truth:

1. `dartscope-core`
2. `dartscope-resolve`
3. `dartscope-parse`
4. `dartscope-index`
5. `dartscope-lints`
6. `dartscope-flutter`
7. `dartscope-json`
8. `dartscope`
9. `dartscope-cli`

Every internal path dependency also carries a crates.io version requirement. The order places all
normal and development dependencies before their consumers.

## GitHub Actions

`.github/workflows/release.yml` packages and tests the workspace on pushes to `main`, version tags,
relevant pull requests, and manual dispatches. Generated `.crate` archives are uploaded as a workflow
artifact.

Publication is never automatic on a normal push or tag. To publish:

1. Create and push the exact tag `v<workspace-version>`.
2. Configure a protected GitHub environment named `crates-io`.
3. Store a crates.io token as the `CARGO_REGISTRY_TOKEN` environment secret.
4. Manually dispatch the Release workflow from that tag with `publish` enabled.
5. Approve the protected environment deployment.

`tools/publish-crates.sh` is idempotent for already-visible crate versions and waits for each
published dependency to become visible through the crates.io registry before continuing.

## After Publication

Verify every crate version on crates.io and docs.rs, create or update the GitHub release from the
matching changelog section, and move new work under `Unreleased`.

Published crates cannot be deleted. For a serious release defect, publish a fixed patch version and
yank the affected version only after documenting the replacement.
