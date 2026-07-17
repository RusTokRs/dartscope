---
id: doc://docs/development/ds-index005-library-ubuntu-diagnostic.md
kind: verification_report
language: en
source_language: en
status: active
---

# DS-INDEX-005 Library Cache Ubuntu Diagnostic

- apply exit: `0`
- install exit: `0`
- focused test exit: `101`

```text
DS-INDEX-005 per-library cache slice applied
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: downloading 4 components

  1.95.0-x86_64-unknown-linux-gnu installed - rustc 1.95.0 (59807616e 2026-04-14)

info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: component rustfmt is up to date
info: downloading component clippy
    Updating crates.io index
 Downloading crates ...
  Downloaded fnv v1.0.7
  Downloaded lazy_static v1.5.0
  Downloaded itoa v1.0.18
  Downloaded foldhash v0.2.0
  Downloaded percent-encoding v2.3.2
  Downloaded thiserror-impl v2.0.18
  Downloaded thiserror v2.0.18
  Downloaded quote v1.0.46
  Downloaded zmij v1.0.21
  Downloaded hashlink v0.11.1
  Downloaded arraydeque v0.5.1
  Downloaded unicode-ident v1.0.24
  Downloaded serde_derive v1.0.228
  Downloaded uriparse v0.6.4
  Downloaded yaml-rust2 v0.11.0
  Downloaded proc-macro2 v1.0.106
  Downloaded serde_core v1.0.228
  Downloaded serde v1.0.228
  Downloaded memchr v2.8.2
  Downloaded hashbrown v0.16.1
  Downloaded serde_json v1.0.150
  Downloaded syn v2.0.118
   Compiling proc-macro2 v1.0.106
   Compiling unicode-ident v1.0.24
   Compiling quote v1.0.46
   Compiling serde_core v1.0.228
   Compiling thiserror v2.0.18
   Compiling zmij v1.0.21
   Compiling serde v1.0.228
   Compiling serde_json v1.0.150
   Compiling foldhash v0.2.0
   Compiling memchr v2.8.2
   Compiling syn v2.0.118
   Compiling itoa v1.0.18
   Compiling fnv v1.0.7
   Compiling lazy_static v1.5.0
   Compiling uriparse v0.6.4
   Compiling hashbrown v0.16.1
   Compiling percent-encoding v2.3.2
   Compiling hashlink v0.11.1
   Compiling arraydeque v0.5.1
   Compiling yaml-rust2 v0.11.0
   Compiling thiserror-impl v2.0.18
   Compiling serde_derive v1.0.228
   Compiling dartscope-core v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-core)
   Compiling dartscope-resolve v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-resolve)
   Compiling dartscope-parse v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-parse)
   Compiling dartscope-index v0.1.0 (/home/runner/work/dartscope/dartscope/crates/dartscope-index)
error[E0716]: temporary value dropped while borrowed
   --> crates/dartscope-index/src/tests/incremental.rs:585:23
    |
585 |       let unresolved = &index.snapshot().graphql_contracts().unresolved_uses[0];
    |                         ^^^^^^^^^^^^^^^^                                       - temporary value is freed at the end of this statement
    |                         |
    |                         creates a temporary value which is freed while still in use
586 | /     assert_eq!(
587 | |         unresolved.reason,
588 | |         DartGraphqlUnresolvedReason::NotVisibleDeclaration
589 | |     );
    | |_____- borrow later used here
    |
help: consider using a `let` binding to create a longer lived value
    |
585 ~     let binding = index.snapshot();
586 ~     let unresolved = &binding.graphql_contracts().unresolved_uses[0];
    |

error[E0716]: temporary value dropped while borrowed
   --> crates/dartscope-index/src/tests/incremental.rs:615:23
    |
615 |       let unresolved = &index.snapshot().graphql_contracts().unresolved_uses[0];
    |                         ^^^^^^^^^^^^^^^^                                       - temporary value is freed at the end of this statement
    |                         |
    |                         creates a temporary value which is freed while still in use
616 | /     assert_eq!(
617 | |         unresolved.reason,
618 | |         DartGraphqlUnresolvedReason::MissingDeclaration
619 | |     );
    | |_____- borrow later used here
    |
help: consider using a `let` binding to create a longer lived value
    |
615 ~     let binding = index.snapshot();
616 ~     let unresolved = &binding.graphql_contracts().unresolved_uses[0];
    |

For more information about this error, try `rustc --explain E0716`.
error: could not compile `dartscope-index` (lib test) due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
```
