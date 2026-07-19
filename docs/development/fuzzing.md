---
id: doc://docs/development/fuzzing.md
kind: development_contract
language: en
source_language: en
status: active
---

# Bounded Fuzzing And Deterministic Properties

DartScope keeps a separate, non-publishable `fuzz/` workspace so libFuzzer and nightly-only tooling do
not enter release crates or the stable Rust 1.95 workspace graph. `dartscope-parse` exposes a
feature-gated, documentation-hidden `fuzzing` bridge that reaches private lexical, directive, and GraphQL
stages without making their intermediate models part of the supported API.

## Targets

The checked-in targets cover:

- lexical masking, including nested comments, raw/triple strings, and unterminated input;
- import/export directives and conditional/combinator forms;
- pubspec YAML and package-config JSON parsing;
- GraphQL operation declarations and client uses;
- path normalization plus package URI validation and resolution.

Every target accepts arbitrary bytes through UTF-8 lossy conversion because the production APIs accept
Rust strings. Inputs are bounded by CI to 4096 bytes. The lexical bridge also checks byte-length and
newline preservation, and all private-stage bridges validate returned source spans.

## Deterministic Property Suite

`crates/dartscope-parse/tests/deterministic_properties.rs` complements libFuzzer with a stable Rust 1.95
integration suite. It uses a checked-in deterministic generator rather than a new property-testing
runtime dependency, so failures reproduce from the reported seed on Linux and Windows.

The suite checks that:

- generated path normalization is idempotent, constructor-stable, and removes every backslash;
- repeated file analysis is byte-for-byte equal and ordered findings remain monotonic by source offset;
- every produced span stays on UTF-8 boundaries and round-trips exact byte, line, and column positions for
  both LF and CRLF source containing non-ASCII text;
- generated package URI resolution is deterministic and normalized;
- dot-segment canonicalization stays within the configured package root, while literal and percent-encoded
  traversal, encoded separators, queries, and fragments are rejected.
- generated direct, prefixed, and re-export combinator matrices preserve `show`/`hide` and
  privacy semantics, while every incremental combinator mutation matches a clean workspace rebuild.

The deterministic suite is bounded and exhaustive only over its generated cases. It does not replace the
malformed-input fuzz corpus or claim analyzer-equivalent parser coverage.

## Toolchain And CI Boundary

CI pins `cargo-fuzz 0.13.2`, `libfuzzer-sys 0.4.13`, and `nightly-2026-07-01`. The normal workspace stays
on Rust 1.95. The Linux-only fuzz job builds all targets and runs each target for a fixed 256 executions
with a five-second per-input timeout and a 2048 MiB RSS limit. This is a bounded panic/regression gate,
not a claim of exhaustive coverage.

The corpus directories contain reviewed valid and malformed seeds. New crash artifacts must be minimized,
converted into a stable regression seed or ordinary unit test, and reviewed before being committed.
Generated `artifacts/`, `coverage/`, and fuzz-local `target/` directories are ignored.

## Local Commands

```bash
cargo +1.95.0 test -p dartscope-parse --test deterministic_properties --locked
cargo +1.95.0 install cargo-fuzz --version 0.13.2 --locked
rustup toolchain install nightly-2026-07-01 --profile minimal
cargo +nightly-2026-07-01 fuzz build lexical_masking
cargo +nightly-2026-07-01 fuzz run lexical_masking -- -runs=256 -max_len=4096 -timeout=5
```

Run the other target names from `fuzz/Cargo.toml` with the same bounded flags. Longer local campaigns are
welcome, but permanent CI intentionally avoids unstable wall-clock thresholds.
