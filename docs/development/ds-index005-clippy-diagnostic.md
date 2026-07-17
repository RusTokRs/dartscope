---
id: doc://docs/development/ds-index005-clippy-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Clippy Diagnostic

- patch apply exit: `0`
- Rust install exit: `0`
- formatting exit: `0`
- Clippy exit: `101`

```text
DS-INDEX-005 semantic invalidation correction applied
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: downloading 5 components

  1.95.0-x86_64-unknown-linux-gnu installed - rustc 1.95.0 (59807616e 2026-04-14)

    Updating crates.io index
 Downloading crates ...
  Downloaded fnv v1.0.7
  Downloaded toml_datetime v1.1.1+spec-1.1.0
  Downloaded serde_spanned v1.1.1
  Downloaded serde v1.0.228
  Downloaded zmij v1.0.21
  Downloaded unicode-ident v1.0.24
  Downloaded yaml-rust2 v0.11.0
  Downloaded toml_parser v1.1.2+spec-1.1.0
  Downloaded winnow v1.0.4
  Downloaded thiserror-impl v2.0.18
  Downloaded proc-macro2 v1.0.106
  Downloaded percent-encoding v2.3.2
  Downloaded uriparse v0.6.4
  Downloaded quote v1.0.46
  Downloaded foldhash v0.2.0
  Downloaded arraydeque v0.5.1
  Downloaded syn v2.0.118
  Downloaded memchr v2.8.2
  Downloaded serde_json v1.0.150
  Downloaded hashbrown v0.16.1
  Downloaded serde_derive v1.0.228
  Downloaded serde_core v1.0.228
  Downloaded toml v1.1.3+spec-1.1.0
  Downloaded thiserror v2.0.18
  Downloaded lazy_static v1.5.0
  Downloaded itoa v1.0.18
  Downloaded hashlink v0.11.1
   Compiling proc-macro2 v1.0.106
   Compiling serde_core v1.0.228
   Compiling quote v1.0.46
   Compiling unicode-ident v1.0.24
   Compiling serde v1.0.228
   Compiling thiserror v2.0.18
   Compiling zmij v1.0.21
   Compiling serde_json v1.0.150
    Checking memchr v2.8.2
    Checking itoa v1.0.18
    Checking lazy_static v1.5.0
   Compiling syn v2.0.118
    Checking fnv v1.0.7
    Checking uriparse v0.6.4
    Checking foldhash v0.2.0
    Checking percent-encoding v2.3.2
    Checking hashbrown v0.16.1
    Checking arraydeque v0.5.1
    Checking hashlink v0.11.1
    Checking winnow v1.0.4
    Checking yaml-rust2 v0.11.0
    Checking toml_parser v1.1.2+spec-1.1.0
    Checking serde_spanned v1.1.1
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking toml v1.1.3+spec-1.1.0
   Compiling serde_derive v1.0.228
   Compiling thiserror-impl v2.0.18
    Checking dartscope-core v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-core)
    Checking dartscope-resolve v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-resolve)
    Checking dartscope-flutter v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-flutter)
    Checking dartscope-json v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-json)
    Checking dartscope-parse v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-parse)
    Checking dartscope-index v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-index)
error: usage of `bool::then` in `filter_map`
   --> crates/dartscope-index/src/incremental.rs:754:10
    |
754 |           .filter_map(|(path, references)| {
    |  __________^
755 | |             references
756 | |                 .iter()
757 | |                 .any(|reference| names.contains(&reference.name))
758 | |                 .then(|| path.clone())
759 | |         })
    | |__________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#filter_map_bool_then
    = note: `-D clippy::filter-map-bool-then` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::filter_map_bool_then)]`
help: use `filter` then `map` instead
    |
754 ~         .filter(|&(path, references)| references
755 +                 .iter()
756 +                 .any(|reference| names.contains(&reference.name))).map(|(path, references)| path.clone())
    |

error: could not compile `dartscope-index` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```
