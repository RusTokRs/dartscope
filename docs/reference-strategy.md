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
- [dart analyze](https://dart.dev/tools/dart-analyze)

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

## Consumer Boundary

Downstream tools can request specific output shapes, but they should not define Dart or
Flutter semantics for DartScope. Consumer-specific mapping belongs in the consuming
project.
