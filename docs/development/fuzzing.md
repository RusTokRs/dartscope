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

## End-To-End Mutation Tests

The libFuzzer targets exercise single parser stages. Two ordinary integration tests cover the whole
pipeline on stable Rust, so they run in every `cargo test --workspace`:

- `crates/dartscope-parse/tests/robustness_mutations.rs` damages nine realistic Dart sources (deleted,
  duplicated and inserted fragments, cut-off files, stray delimiters and quotes, byte-order marks, CR and
  CRLF line ends, non-ASCII text) and analyzes every result with `analyze_file_with_references`. It
  requires that nothing panics and that every reported span describes the text: offsets inside the source
  and on character boundaries, ordered lines, and lines and columns that agree with an independent count.
- `crates/dartscope-index/tests/robustness_mutations.rs` applies short random sequences of edits,
  removals and re-additions to a two-file workspace through the incremental API. After every step the
  snapshot must equal a stateless analysis (project, URI graph, part links, reference resolutions), and
  definition and reference queries over every reference must not panic.

Both tests catch every panic, group the failures by source location, and shrink each to a reproducer of a
few characters before reporting, so one run lists every distinct failure. The generators are
deterministic (xorshift with fixed seeds). The defaults are small; a longer hunt turns the knobs up and
should use a release build:

```bash
DARTSCOPE_MUTATION_ROUNDS=6000 DARTSCOPE_MUTATION_SEED=3 \
  cargo test --release -p dartscope-parse --test robustness_mutations
DARTSCOPE_MUTATION_ROUNDS=1500 DARTSCOPE_MUTATION_SEED=3 \
  cargo test --release -p dartscope-index --test robustness_mutations
```

A failure found this way is fixed in the library and its reproducer becomes an ordinary regression test
next to the code it concerns. The first campaign found two panics on a non-ASCII character in code that
is not valid Dart (a slice at a byte that is not on a character boundary) and an incremental-index
staleness (a cached resolution kept the full span of its target declaration, and the invalidation
compared only line spans and top-level declarations).

### Differential check when the reference passes are restructured

The mutation corpus doubles as a differential test. The reference passes were rewritten from per-token
scans to lookup structures (`FileFacts`, 2026-10-01) with the requirement that the output stays identical,
not just plausible. The check that was used, and that suits any such rewrite:

1. Write an example program (not committed) that replays the seeds and the mutation operators of
   `robustness_mutations.rs` and prints `salt:seed:mutant:hash` for every mutant, where the hash covers
   `format!("{:?}", analyze_file_with_references(..))`, plus a `--dump salt:seed:mutant` mode that prints the
   source and the pretty `Debug` output of one mutant. Add a few generated large shapes as seeds (many
   classes, widgets with locals and closures, one method with thousands of statements, one long expression).
2. Build it at the previous commit (`git worktree add`) and at the new tree, run both and `diff` the output.
   Any difference is a defect of the rewrite; diff the two `--dump` outputs of the first mismatch.

The rewrite of `FileFacts` produced no difference in 15,652 mutants of 13 seeds, and none in a longer
campaign of 151,326 mutants of 21 seeds. Each structure additionally has an equivalence test against the scan
it replaces (the `linear` modules in the tests of `interval_index.rs`, `source_structure.rs`,
`declaration_tables.rs`, `binding_index.rs` and `lexical_reads.rs`).

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
