# Rust Code Standards For Agents

This document defines how agents write and refactor Rust in DartScope. It is mandatory
for production code, tests, examples, and public APIs.

The goal is not clever code or maximum abstraction. The goal is code whose name,
location, ownership, error behavior, and reason for existence are obvious during review.

## Authority

Use these sources in order:

1. [The Rust Reference](https://doc.rust-lang.org/reference/)
2. [The Rust Style Guide](https://doc.rust-lang.org/style-guide/)
3. [The Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html)
4. [The Rust Programming Language](https://doc.rust-lang.org/book/)
5. [The rustdoc book](https://doc.rust-lang.org/rustdoc/)
6. [Clippy documentation](https://doc.rust-lang.org/clippy/)
7. this project standard and existing local architecture

Do not invent a Rust convention from memory when an official source defines it. If
official guidance permits multiple designs, use the design already established in the
owning crate. Record a deliberate exception next to the code or in the development plan.

## Non-Negotiable Rules

- Run `rustfmt`; do not hand-format against it.
- Use the smallest owning module. Do not add unrelated logic to a convenient large file.
- Give every item one clear responsibility.
- Prefer direct, domain-specific names over generic or decorative names.
- Preserve errors and uncertainty. Do not replace a meaningful result with a panic,
  silent fallback, empty vector, or guessed value.
- Keep public APIs and serialized types compatibility-sensitive.
- Add a focused test before fixing a bug or changing observable behavior.
- Refactor an oversized boundary before adding another independent responsibility.
- Do not add `unsafe` without an explicit task, a documented invariant, and review.

## Naming

Follow the Rust Style Guide casing exactly:

| Item | Form | Example |
| --- | --- | --- |
| crate/package | `kebab-case` package, `snake_case` crate path | `dartscope-parse`, `dartscope_parse` |
| module/file | `snake_case` | `uri_graph.rs` |
| struct/enum/trait/type | `UpperCamelCase` | `DartUriGraph` |
| enum variant | `UpperCamelCase` | `MissingTarget` |
| function/method/local/field | `snake_case` | `resolve_package_uri` |
| constant/static | `SCREAMING_SNAKE_CASE` | `SUPPORTED_CONFIG_VERSION` |
| lifetime | short lowercase | `'a`, `'source` when meaning matters |

Initialisms are words, not all-caps fragments: use `Json`, `Uri`, `Graphql`, `Http`, and
`Id` inside type names. Preserve an external spelling only when it is part of a protocol
or serialized contract.

### Choosing The Words

- Name a type with a noun that states what it represents: `PackageConfigAnalysis`.
- Name an action with a verb and its domain object: `parse_package_config`.
- Name predicates with `is_`, `has_`, `can_`, `should_`, or `supports_`.
- Name collections with a plural noun: `candidate_paths`, not `candidate_path_list`.
- Name a count with `_count` only when the value is not already obviously a collection
  length. Include the unit in numeric names: `timeout_ms`, `byte_start`.
- Getters use the field/domain noun, not `get_foo`, unless `get` is meaningful to the
  operation rather than mere access.
- Conversions follow Rust API Guidelines: `as_` borrows, `to_` allocates or computes,
  `into_` consumes, `from_` constructs when a standard `From`/`TryFrom` implementation
  is not sufficient.
- Implement `From` and `TryFrom`, not direct `Into` and `TryInto` implementations.
- Use `new` for the primary unsurprising constructor. Use `with_*` for builder-style
  optional configuration and a domain verb such as `open` or `parse` when construction
  performs that operation.

Avoid names that hide responsibility:

```text
data, info, item, thing, stuff, object, manager, processor, handler,
helper, helpers, util, utils, common, misc, temp, result_data,
do_work, process_data, handle_item
```

These words are allowed only when they are the actual domain term. Otherwise name the
owned concept or operation. Do not add suffixes such as `Impl`, `Base`, `Core`, `New`,
or `V2` unless they distinguish a real public role or protocol version.

Name-length review trigger: more than four conceptual words or roughly 40 characters.
Do not shorten a precise domain term into an obscure abbreviation. Instead ask whether
the item owns too many concepts or sits in the wrong module.

Avoid repeating context already supplied by the type or module:

```rust
// Prefer inside impl PackageConfigAnalysis:
fn validate(&self) { /* ... */ }

// Avoid:
fn validate_package_config_analysis(&self) { /* ... */ }
```

Public free functions may keep enough context to remain clear when imported directly.

## Functions And Methods

A function should perform one operation at one abstraction level.

Project review triggers:

| Measure | Target | Mandatory action |
| --- | --- | --- |
| executable lines | usually <= 40 | above 100: split or document why it is declarative |
| cognitive complexity | usually <= 20 | above Clippy default 25: refactor before feature work |
| parameters | usually <= 5 | above Clippy default 7: use a request/options/context type |
| boolean parameters | 0 or 1 | use an enum/options type when modes can be named |
| nesting | usually <= 3 levels | extract a named decision or use early return |

These are review triggers, not permission to create meaningless one-line wrappers.
Extract a function only when the extracted block has a stable name, contract, or
independent test value.

Function rules:

- Put validation and early exits first; keep the main path visually direct.
- Prefer `match` when all states matter and `if let`/`let else` for one relevant branch.
- Return a value or result instead of mutating an out-parameter, except when reusing a
  caller-owned buffer is the purpose of the API.
- Use a method when there is a clear receiver. Use a free function when no type owns the
  operation or when it composes multiple domains.
- Do not encode modes with multiple booleans. Use an enum or options struct.
- Do not clone merely to silence the borrow checker. State why ownership is needed or
  change the boundary to borrow.
- Prefer clear loops over dense iterator chains when state, branching, or diagnostics
  are easier to follow imperatively.
- Do not combine parsing, resolution, filesystem I/O, and serialization in one function.
- Keep orchestration functions short: call named stages and assemble their results.

## Files, Modules, And Crates

Split by responsibility, not by line count alone. Line count tells an agent when to
inspect the design; the module boundary must still have a meaningful name.

Project review triggers for production Rust:

| File size | Required response |
| --- | --- |
| up to 500 lines | normal review |
| 501-800 lines | identify the next extraction boundary before adding a new responsibility |
| above 800 lines | no new feature until the file has a recorded split plan |
| above 1,200 lines | perform the behavior-preserving split before feature work |

Test files may be longer when they are a readable fixture matrix, but above 1,000 lines
split tests by behavior (`directives`, `declarations`, `graphql`, `routes`) rather than
using names such as `tests_2` or `more_tests`.

Use these boundaries:

- a module owns one domain capability or one internal stage;
- `lib.rs` contains crate documentation, public module declarations, re-exports, and
  thin composition, not every implementation;
- private implementation modules stay private; re-export only intentional public API;
- sibling modules communicate through the narrowest useful types;
- avoid `common.rs` and `utils.rs`; use the capability name such as `source_lines.rs` or
  `namespace.rs`;
- create a new crate only for an independently usable capability, optional dependency
  boundary, process/I/O boundary, or stable ownership boundary, not because a file is
  large.

Do not split into arbitrary numbered files. A good module name answers both "what does
this own?" and "where should the next related change go?".

## When Refactoring Is Required

Refactor before adding a feature when any of these is true:

- the target file is above 1,200 production lines;
- the changed function exceeds 100 lines or cognitive complexity 25;
- the new code introduces a third independent reason for the module to change;
- the same parsing, validation, sorting, or error mapping logic appears a third time;
- tests need private knowledge from unrelated stages;
- a feature would add another backend/framework/consumer condition to core logic;
- naming the new item precisely requires joining multiple domain concepts.

Refactoring procedure:

1. Add or confirm characterization tests for existing behavior.
2. Write the intended module map before moving code.
3. Move one responsibility at a time and preserve public paths with re-exports.
4. Do not mix broad behavior changes into the move.
5. Run focused tests after each boundary, then the full workspace checks.
6. Compare public Rust and JSON output when public types are involved.
7. Delete the old implementation only after all callers use the new owner.

A refactor is complete only when ownership is clearer. Fewer lines in one file with the
same coupling hidden across several files is not an improvement.

## Reference Example: DS-MAINT-001

The DartScope parser and project index are the reference implementation for this
standard. They were split without changing public functions, JSON fields, diagnostics,
confidence, paths, ordering, or fixtures.

| Crate | Module | Owns |
| --- | --- | --- |
| `dartscope-parse` | `analysis` | file/project orchestration only |
| `dartscope-parse` | `source_lines` | CRLF-aware source-line offsets and diagnostic paths |
| `dartscope-parse` | `namespace` | imports, exports, namespace controls |
| `dartscope-parse` | `declarations` | top-level declarations and identifier helpers |
| `dartscope-parse` | `graphql` | GraphQL document/use extraction |
| `dartscope-parse` | `pubspec` | pubspec analysis and YAML subset helpers |
| `dartscope-parse` | `declaration_inventory` | structural declaration, member, and local-symbol inventory |
| `dartscope-parse` | `invocations` | parser-independent call targets, arguments, map entries, and source evidence |
| `dartscope-flutter` | `conventions` | optional Flutter and ecosystem interpretation of generic facts |
| `dartscope-index` | `uri_graph` | URI graph construction and package URI resolution |
| `dartscope-index` | `parts` | part ownership validation |
| `dartscope-index` | `graphql` | cross-file GraphQL visibility and contract linking |
| `dartscope-index` | `paths` | private path normalization primitives |

Both `lib.rs` files are intentionally thin: module declarations, public re-exports, and
crate documentation only. Tests are split by behavior under `src/tests/`. Treat this as
a pattern for future capability splits, not a requirement to copy these exact names.

## Public API Design

- Make invalid states difficult to construct. Prefer enums and validated newtypes over
  strings and related booleans.
- Use meaningful error types implementing `Debug`, `Display`, and `std::error::Error`.
- Public library functions do not return `String` as an error when callers need to
  classify failure.
- Implement common traits (`Debug`, `Clone`, `Eq`, `Hash`, `Default`, `Display`) when
  their semantics are honest and useful.
- Use `#[must_use]` for values whose ignored result is probably a bug.
- Use `#[non_exhaustive]` deliberately for public enums/structs expected to grow; do not
  add it mechanically.
- Borrow inputs when the function does not need ownership; return owned analysis data
  when it must outlive the input.
- Keep feature flags additive, named for capabilities, and independent where promised.
- Preserve Serde compatibility with defaults for additive fields when possible.
- A breaking Rust or JSON change requires a migration note and contract fixture.

## Errors, Panics, And Unsafe Code

- Invalid user input, unsupported syntax, missing files, and external data errors return
  `Result` or DartScope diagnostics. They do not panic.
- Error messages are concise, lowercase, and have no trailing punctuation unless they
  contain multiple sentences.
- Preserve the error source with `#[source]`/`source()` where it helps callers.
- `unwrap()` and `expect()` are allowed in tests and examples. In production they require
  a local invariant that is obvious or explained in a short comment.
- Do not use `panic!`, `todo!`, or `unimplemented!` for reachable production paths.
- Never ignore a `Result` with `let _ =` unless best-effort behavior is the documented
  contract and the reason is stated.
- New `unsafe` code requires an explicit task. Every unsafe function documents
  `# Safety`; every unsafe block has a `// SAFETY:` comment proving its invariants.

Do not enable the entire Clippy `restriction` group. Official Clippy documentation
warns that restriction lints can conflict. Select individual lints only after the
existing workspace is clean under them.

## Documentation And Comments

- Public crates and modules have `//!` documentation explaining purpose and boundaries.
- Public items have `///` documentation when their contract is not fully obvious.
- Start rustdoc with one concise summary sentence.
- Add `# Errors`, `# Panics`, and `# Safety` sections when applicable.
- Add an example to the primary public workflow; do not add an example that only repeats
  the signature.
- Comments explain why, invariants, protocol rules, or non-obvious tradeoffs. Do not
  narrate assignments or restate the code.
- Link official behavior in docs or tests when a parser/resolver rule comes from a
  specification.
- Do not use comments to excuse poor structure. Refactor first when a name can express
  the concept.

## Tests

- Name tests as behavior: `rejects_duplicate_package_names`, not `test_parser_2`.
- One test proves one behavior, though it may assert all fields of that behavior.
- Every bug fix starts with a regression test that fails for the original reason.
- Heuristics require a positive and nearby negative case.
- Assert normalized paths, exact spans, confidence, diagnostics, and deterministic
  ordering where they are part of the contract.
- Use fixtures for multi-file/project behavior; use inline source for a small syntax
  edge case.
- Avoid large setup inside a test. Extract a domain-named fixture builder, not a generic
  helper bag.
- Production code must not exist only to make a test inspect private implementation.
  Test observable behavior or a real internal boundary.

## Dependencies And Official Behavior

Before adding a dependency, record:

- the capability it owns and why the standard library/current workspace is insufficient;
- official repository/documentation;
- license, MSRV, maintenance state, feature flags, and transitive cost;
- whether it performs I/O, runs a process, uses unsafe code, or changes deployment;
- the adapter/port that prevents it from becoming the public model.

For Dart and Flutter semantics, follow `docs/reference-strategy.md`. A downstream
consumer request may motivate a feature but cannot redefine language behavior.

## Agent Review Checklist

Before editing:

- Can I name the owning module in one domain phrase?
- Is the target file/function already over a refactor trigger?
- Is the behavior backed by an official source or explicitly a heuristic?
- Can I reproduce the change with a focused test?

Before finishing:

- Are names shorter and clearer without losing domain meaning?
- Does each changed function operate at one abstraction level?
- Did I leave generic `helper`, `manager`, `data`, or `process` names?
- Did I introduce a panic, silent fallback, unnecessary clone, or boolean mode?
- Are public errors, docs, traits, spans, and serialization complete?
- Should any moved implementation remain private?
- Do focused tests and all required workspace checks pass?

If any answer is unclear, do not call the task complete. Reduce the change or record the
required refactor in the ordered roadmap.
