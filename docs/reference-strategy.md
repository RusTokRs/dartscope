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
- [Internationalizing Flutter apps](https://docs.flutter.dev/ui/accessibility-and-internationalization/internationalization)
- [MaterialApp API](https://api.flutter.dev/flutter/material/MaterialApp-class.html)
- [WidgetsApp API](https://api.flutter.dev/flutter/widgets/WidgetsApp-class.html)
- [Navigator API](https://api.flutter.dev/flutter/widgets/Navigator-class.html)

## Implementation References

Implementation references may be used after the official behavior is identified:

- Dart analyzer behavior and diagnostics
- `tree-sitter-dart` or other parser grammars
- optional analyzer bridges that call Dart tooling
- `custom_lint`, `build_runner`, `melos`, and similar ecosystem tools

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

## Real-Project Feedback Loop

Use at least one real Flutter frontend as a calibration target outside this repository:

```powershell
cargo run -p dartscope-cli -- analyze-project D:\path\to\frontend
```

For each pass, record what was correct, what was missed, what was falsely inferred, and
where a diagnostic would be better than a confident finding. Each reusable case should
be reduced to a small fixture in DartScope before broadening the parser or Flutter
heuristics.

Project URI resolution must distinguish indexed source knowledge from Dart package
configuration knowledge. A `package:` URI whose package is absent from the loaded
pubspecs is unindexed, not proven invalid. Full package resolution should consume the
official package configuration in a later phase rather than infer dependency locations.

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
