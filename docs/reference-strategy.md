# Reference Strategy

DartScope must be built from documented Dart and Flutter behavior. Implementation
shortcuts are allowed in early releases, but every supported construct should be
traceable to an official source or explicitly marked as a heuristic.

## Source Classes

```text
normative       official specifications, official docs, official API docs
behavioral      Dart analyzer, dart/pub/flutter tool behavior
implementation  parser crates, analyzer bridges, tree-sitter grammars
ecosystem       community tools and conventions
consumer        downstream integration needs
```

Normative and behavioral sources define expected behavior. Implementation and ecosystem
sources can guide coverage and ergonomics, but they should not silently redefine Dart or
Flutter semantics.

## Required Official References

Dart language and libraries:

- [Dart language specification](https://dart.dev/resources/language/spec)
- [Dart language documentation](https://dart.dev/language)
- [Libraries and imports](https://dart.dev/language/libraries)
- [Class modifiers](https://dart.dev/language/class-modifiers)
- [Class modifier reference](https://dart.dev/language/modifier-reference)
- [Mixins](https://dart.dev/language/mixins)
- [Methods](https://dart.dev/language/methods)
- [Constructors](https://dart.dev/language/constructors)
- [Primary constructors](https://dart.dev/language/primary-constructors)
- [Operators](https://dart.dev/language/operators)
- [Extension methods](https://dart.dev/language/extension-methods)
- [Extension types](https://dart.dev/language/extension-types)

Dart packages and tooling:

- [pubspec file documentation](https://dart.dev/tools/pub/pubspec)
- [Package dependencies](https://dart.dev/tools/pub/dependencies)
- [Package layout conventions](https://dart.dev/tools/pub/package-layout)
- [Resolving package URIs](https://api.dart.dev/dart-isolate/Isolate/resolvePackageUriSync.html)
- [Package Configuration File v2 specification](https://github.com/dart-lang/language/blob/main/accepted/2.8/language-versioning/package-config-file-v2.md)
- [`package_config` reference implementation](https://github.com/dart-lang/tools/tree/main/pkgs/package_config)
- [dart analyze](https://dart.dev/tools/dart-analyze)
- [`part_of_non_part` analyzer diagnostic](https://dart.dev/tools/diagnostics/part_of_non_part)
- [`part_of_different_library` analyzer diagnostic](https://dart.dev/tools/diagnostics/part_of_different_library)
- [`ambiguous_import` analyzer diagnostic](https://dart.dev/tools/diagnostics/ambiguous_import)
- [`ambiguous_export` analyzer diagnostic](https://dart.dev/tools/diagnostics/ambiguous_export)

Flutter framework:

- [Flutter navigation and routing](https://docs.flutter.dev/ui/navigation)
- [Flutter assets and images](https://docs.flutter.dev/ui/assets/assets-and-images)
- [Flutter pubspec asset options](https://docs.flutter.dev/tools/pubspec)
- [Flutter asset transformation](https://docs.flutter.dev/ui/assets/asset-transformation)
- [Internationalizing Flutter apps](https://docs.flutter.dev/ui/internationalization)
- [MaterialApp API](https://api.flutter.dev/flutter/material/MaterialApp-class.html)
- [WidgetsApp API](https://api.flutter.dev/flutter/widgets/WidgetsApp-class.html)
- [Navigator API](https://api.flutter.dev/flutter/widgets/Navigator-class.html)
- [`go_router` package](https://pub.dev/packages/go_router)

The Flutter pubspec options define `path`, user-defined `flavors`, and the valid asset
`platforms` values `android`, `ios`, `web`, `linux`, `macos`, and `windows`. DartScope
therefore preserves non-empty flavor names without inventing a closed vocabulary and
diagnoses platform values outside the documented set. The asset-transformation
documentation defines ordered transformer packages with optional scalar arguments;
DartScope preserves transformer order because Flutter applies transformations
sequentially. The pubspec localization switch is `flutter.generate`; richer localization
configuration is read from explicit `l10n.yaml` inputs in the optional Flutter layer. DartScope
follows the documented `arb-dir`, template file, generated output file, output class, and
output-directory defaults. ARB
message keys exclude `@message` metadata and `@@locale` metadata.

## Implementation References

Implementation references may be used after the official behavior is identified:

- [`yaml-rust2` 0.11.0](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/) as the accepted private pubspec YAML backend;
- [`yaml-rust2::parser::Event`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/parser/enum.Event.html) for explicit alias and document events;
- [`yaml-rust2::scanner::Marker`](https://docs.rs/yaml-rust2/0.11.0/yaml_rust2/scanner/struct.Marker.html) for byte-indexed source evidence;
- Dart analyzer behavior and diagnostics;
- `tree-sitter-dart` or other parser grammars;
- optional analyzer bridges that call Dart tooling;
- `custom_lint`, `build_runner`, `melos`, and similar ecosystem tools.

The YAML backend is an implementation mechanism, not the source of pubspec semantics.
Official Dart/pub documentation continues to define accepted fields and meanings.
DartScope's adapter must reject aliases and merge keys according to its documented
support policy even when the backend can parse them.

When DartScope adopts behavior from an implementation or ecosystem source, the code or
test fixture should make the status clear: official behavior, observed tool behavior,
or DartScope heuristic.

Framework conventions outside the Flutter SDK, such as Riverpod widget base classes,
are ecosystem-supported behavior. They require reduced fixtures and should stay
optional conventions rather than changing the pure Dart core model.

## Fixture Rule

Every new supported syntax or Flutter convention should have a fixture that names the
reference class it relies on. Heuristic fixtures should assert diagnostics or confidence
metadata instead of pretending the result is complete.

Every parser fixture should identify its source class in a nearby test name, comment,
or support-ledger entry. A normative fixture needs an official specification or API
reference. An ecosystem fixture needs the package and supported version range. A
heuristic fixture needs both a positive case and a nearby negative case.

## Current Support Ledger

| Surface | Source class | Current status | Important limit |
| --- | --- | --- | --- |
| import, export, part, part-of | normative | implemented heuristic backend | not a full lexer/AST |
| declarations and type members | normative | top-level declarations, traditional constructors, methods, fields, accessors, operators, and local ownership implemented | Dart 3.13 primary and concise constructors are diagnostic-only pending language-version-aware parsing |
| lexical bindings and unqualified variable access | normative operator semantics plus heuristic scope intervals | parameters, block locals, reads, plain writes, and paired compound/increment read-write facts implemented | closure/loop/catch and pattern bindings, member/index writes, destructuring, and initializer ordering remain deferred |
| class modifiers and mixin class | normative | implemented | validity combinations not diagnosed |
| pubspec dependency sections | normative YAML/pub behavior | typed model and hardened subset | `yaml-rust2` adapter selected but not integrated |
| Flutter pubspec asset declarations | normative Flutter docs | paths, opaque flavors, validated platforms, ordered transformers implemented | selector item spans await marked-event adapter |
| package configuration v2 | normative format | implemented | generated metadata and overlap validation incomplete |
| conditional URI selection | normative | implemented | caller must provide environment |
| GraphQL documents in Dart strings | ecosystem heuristic | implemented | not Dart or Flutter language semantics |
| `GoRoute` and Riverpod widget bases | ecosystem convention | implemented heuristic | package/version matrix not yet explicit |
| Flutter widget, route, asset, and localization conventions | official API plus ecosystem convention | derived in optional `dartscope-flutter` from generic facts | complex/dynamic expressions remain heuristic |
| Flutter asset declarations and localization catalogs | normative Flutter docs plus explicit-input analysis | direct literal uses linked to nearest pubspec; `l10n.yaml` defaults and ARB keys parsed | directory declarations cover direct children; dynamic package expressions remain unresolved; no filesystem existence check or generated-code execution |

## Real-Project Feedback Loop

Use at least one real Flutter frontend as a calibration target outside this repository:

```powershell
cargo run -p dartscope-cli -- analyze-project D:\path\to\frontend
```

For each pass, record what was correct, what was missed, what was falsely inferred, and
where a diagnostic would be better than a confident finding. Each reusable case should
be reduced to a small fixture in DartScope before broadening the parser or Flutter
heuristics.

Project URI resolution distinguishes indexed source knowledge from Dart package
configuration knowledge. When a nearest valid `.dart_tool/package_config.json` is
provided, DartScope uses it. Without one, a `package:` URI whose package is absent from
loaded pubspecs is unindexed, not proven invalid.

Configurable import/export URI selection follows the Dart language specification:
conditions are looked up in the compilation environment, a condition without `==`
compares against `"true"`, configuration clauses are evaluated in source order, and
the default URI is used when nothing matches.

Part ownership follows the analyzer distinction between a missing part file, a target
without `part of`, and a target that names a different owner. A package target outside
the loaded index is unresolved, not proven missing.

## Consumer Boundary

Downstream tools can request specific output shapes, but they should not define Dart or
Flutter semantics for DartScope. Consumer-specific mapping belongs in the consuming
project.

## Official Flutter Theme Facts

Theme construction and application support is normative and follows the official `ThemeData`,
`MaterialApp`, `Theme`, and `AnimatedTheme` API documentation. The supported subset and explicit
non-evaluation boundary are recorded in `docs/development/flutter-themes.md`. Ecosystem theme
packages are not implied by this official support.

