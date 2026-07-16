from pathlib import Path
import shutil
import subprocess
import textwrap


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected one match in {path}, found {count}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1))


subprocess.run(['git', 'config', 'core.autocrlf', 'false'], check=True)
subprocess.run(['git', 'reset', '--hard', 'HEAD'], check=True)

payload = Path('.github/payloads')
destination = Path('crates/dartscope-parse/src/declaration_inventory')
destination.mkdir(parents=True, exist_ok=True)
for source_name, target_name in [
    ('declaration_inventory_mod.rs', 'mod.rs'),
    ('declaration_inventory_scanner.rs', 'scanner.rs'),
    ('declaration_inventory_syntax.rs', 'syntax.rs'),
]:
    shutil.move(payload / source_name, destination / target_name)

subprocess.run(
    [
        'git',
        'apply',
        '--ignore-space-change',
        '--ignore-whitespace',
        str(payload / 'parse006.patch'),
    ],
    check=True,
)

replace_once(
    'README.md',
    textwrap.dedent('''
    - The first parser backend is line-oriented and conservative. It does not yet provide
      a complete Dart AST or type system; lexical masking prevents findings inside comments
      and strings, but complex annotations and multi-line declarations remain limited.
    ''').lstrip(),
    textwrap.dedent('''
    - The first parser backend is conservative and does not provide a complete Dart AST or
      type system. Lexical masking prevents findings inside comments and strings, while a
      structural declaration pass records complete spans for supported declarations. Complex
      metadata layouts, patterns, records, and newer language-versioned syntax remain limited.
    ''').lstrip(),
)
replace_once(
    'README.md',
    textwrap.dedent('''
    - Declaration coverage is top-level only. Methods, constructors, fields, getters,
      setters, operators, and local symbols are roadmap work.
    ''').lstrip(),
    textwrap.dedent('''
    - Declaration inventory covers top-level declarations plus class, mixin, enum,
      extension, and extension-type methods, traditional constructors, fields, getters,
      setters, operators, and local variables. Declarations carry stable hierarchical symbol
      IDs and optional complete declaration spans. Dart 3.13 primary and concise constructors
      currently produce explicit diagnostics instead of heuristic symbols.
    ''').lstrip(),
)

replace_once(
    'docs/reference-strategy.md',
    '- [Mixins](https://dart.dev/language/mixins)\n',
    textwrap.dedent('''
    - [Mixins](https://dart.dev/language/mixins)
    - [Methods](https://dart.dev/language/methods)
    - [Constructors](https://dart.dev/language/constructors)
    - [Primary constructors](https://dart.dev/language/primary-constructors)
    - [Operators](https://dart.dev/language/operators)
    ''').lstrip(),
)
replace_once(
    'docs/reference-strategy.md',
    '| class, mixin, enum, extension, extension type, typedef | normative | top-level slice | members not indexed |\n',
    '| declarations and type members | normative | top-level declarations, traditional constructors, methods, fields, accessors, operators, and local ownership implemented | Dart 3.13 primary and concise constructors are diagnostic-only pending language-version-aware parsing |\n',
)

replace_once(
    'docs/development/dartscope-library-plan.md',
    '| `blocked` | Cannot proceed until the named blocker changes |\n| `deferred` | Explicitly outside the current release target |\n',
    '| `blocked` | Cannot proceed until the named blocker changes |\n| `research` | Investigation is recorded; implementation scope and acceptance are not yet committed |\n| `deferred` | Explicitly outside the current release target |\n',
)

old_parse = textwrap.dedent('''
### DS-PARSE-006: Complete Declaration Inventory

Status: ready. Priority: P1. Prerequisite: DS-PARSE-005.

Add normalized methods, constructors, fields, getters, setters, operators, and local
scope ownership. Add enclosing symbol IDs and declaration spans covering the complete
declaration, not only its first line. Include modern primary and concise constructor
syntax only after official language-version references are recorded.

Acceptance:

- fixtures cover class, mixin, enum, extension, and extension-type members;
- declarations have stable parent relationships;
- constructor calls are not reported as declarations;
- unsupported recent syntax emits a diagnostic rather than a fabricated symbol.

''').lstrip()
new_parse = textwrap.dedent('''
### DS-PARSE-006: Complete Declaration Inventory

Status: verified. Priority: P1. Prerequisite: DS-PARSE-005.

Implemented slices:

1. Normalized methods, traditional constructors, fields, getters, setters, operators, and
   local variables for class, mixin, enum, extension, and extension-type bodies.
2. Deterministic hierarchical `symbol_id` and `parent_symbol_id` values for top-level,
   member, and local declarations.
3. Additive optional `declaration_span` values covering the complete supported declaration;
   the existing `span` remains the compatibility anchor for the declaration's source line.
4. Multiple fields in one declaration and both inferred and explicitly typed local variables.
5. Body-depth filtering that excludes constructor calls and other expressions from the
   declaration inventory.
6. Explicit `unsupported_primary_constructor` and `unsupported_concise_constructor`
   diagnostics for Dart 3.13 syntax until language-version-aware parsing is implemented.

Verification:

- focused fixtures cover every required owner and member category, full spans, stable parents,
  local ownership, multiple fields, typed locals, and nearby constructor-call negatives;
- exact Rust 1.95 workspace tests, all-feature tests, formatting, and Clippy with warnings denied
  pass before finalization;
- the finalization workflow repeats the repository checks before committing to `main`.

Acceptance:

- fixtures cover class, mixin, enum, extension, and extension-type members;
- declarations have stable parent relationships;
- constructor calls are not reported as declarations;
- unsupported recent syntax emits a diagnostic rather than a fabricated symbol.

''').lstrip()
replace_once('docs/development/dartscope-library-plan.md', old_parse, new_parse)

replace_once(
    'docs/development/dartscope-library-plan.md',
    'Status: planned. Priority: P1. Prerequisites: DS-PARSE-005, DS-JSON-001.\n\nMigration sequence:\n',
    'Status: ready. Priority: P1. Prerequisites: DS-PARSE-005, DS-JSON-001.\n\nMigration sequence:\n',
)

release_marker = textwrap.dedent('''
### DS-RELEASE-001: Publishable 0.1 Release

Status: planned. Priority: P3. Prerequisites: DS-JSON-001, DS-CLI-002.

Add complete package metadata, rustdoc coverage, changelog, security policy, crate
publish order, `cargo package` checks, release CI, and an explicit support matrix for
Rust, Dart, Flutter, and ecosystem conventions.

## Completed Tasks
''').lstrip()
release_with_research = textwrap.dedent('''
### DS-RELEASE-001: Publishable 0.1 Release

Status: planned. Priority: P3. Prerequisites: DS-JSON-001, DS-CLI-002.

Add complete package metadata, rustdoc coverage, changelog, security policy, crate
publish order, `cargo package` checks, release CI, and an explicit support matrix for
Rust, Dart, Flutter, and ecosystem conventions.

### DS-COMPAT-001: Upstream Compatibility Radar

Status: research. Priority: P3. Not on the current 0.1 critical path.

Research scope:

1. Evaluate a CI-only official Dart analyzer oracle that compares DartScope's declared
   capabilities rather than requiring internal AST equality.
2. Define a version matrix where current stable can block changes, beta reports early drift,
   and dev/main channels are non-blocking compatibility radar.
3. Design a reduced conformance corpus, differential reports, scheduled release detection,
   and GitHub issue or draft-PR creation.
4. Define automation safety boundaries: CI may detect, minimize, report, and propose changes,
   but must not rewrite semantic parser rules, accept goldens, weaken assertions, or merge fixes.
5. Evaluate runtime cost, cache strategy, network and token permissions, upstream API stability,
   and the security boundary of any `tools/dart-oracle` prototype.

Research exit:

- official source and behavioral reference map is recorded;
- a bounded `tools/dart-oracle` exchange contract is prototyped or rejected with evidence;
- stable, beta, and development-channel failure policies are documented;
- implementation is split into reviewable follow-up tasks with explicit acceptance criteria.

## Completed Tasks
''').lstrip()
replace_once('docs/development/dartscope-library-plan.md', release_marker, release_with_research)

completed_cli = textwrap.dedent('''
### DS-CLI-002: CLI Contract And Integration Tests

Status: verified.

All seven command families have stable help, version, exit-code, stdout/stderr, malformed
input, environment option, filesystem discovery, paths-with-spaces, generated-directory,
and symlink behavior covered by process-level tests on Linux and Windows.

''').lstrip()
completed_parse = completed_cli + textwrap.dedent('''
### DS-PARSE-006: Complete Declaration Inventory

Status: verified.

Supported declarations now include type members and callable-local variables with stable
hierarchical IDs, parent relationships, and complete optional declaration spans. Traditional
constructors are distinguished from calls; unsupported Dart 3.13 constructor forms emit
explicit diagnostics rather than fabricated symbols.

''').lstrip()
replace_once('docs/development/dartscope-library-plan.md', completed_cli, completed_parse)

old_next = textwrap.dedent('''
Implement `DS-PARSE-006` complete declaration inventory next. The stable tool boundary is
verified through `DS-JSON-001` and `DS-CLI-002`; methods, constructors, fields, accessors,
operators, local ownership, complete declaration spans, and stable parent relationships are
now the first ready semantic-model task.
''').lstrip()
new_next = textwrap.dedent('''
Implement `DS-FLUTTER-002` next. The parser now exposes stable generic declaration ownership;
the next architectural step is a parser-independent invocation model so Flutter convention
extraction can move behind the optional `dartscope-flutter` boundary without changing pure Dart
semantics. `DS-COMPAT-001` remains recorded as research and is intentionally deferred until the
current implementation queue is complete.
''').lstrip()
replace_once('docs/development/dartscope-library-plan.md', old_next, new_next)

(payload / 'parse006.patch').unlink()
