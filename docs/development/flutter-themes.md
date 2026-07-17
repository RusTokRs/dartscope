---
id: doc://docs/development/flutter-themes.md
kind: development_note
language: en
source_language: en
status: active
---

# Official Flutter Theme Facts

DartScope exposes official Material theme facts through `dartscope-flutter` without evaluating a
widget tree or resolving Dart types. The convention layer consumes normalized imports and
invocations produced by `dartscope-parse`.

## Public API

- `derive_flutter_theme_facts(&DartFileAnalysis)` derives facts for one parsed file.
- `extract_flutter_theme_facts(&DartProjectAnalysis)` aggregates facts deterministically by path and
  source position.
- `FlutterThemeFacts` separates construction sites from application slots.

This API is separate from the v1 `FlutterInventory` JSON contract, so the existing inventory shape
and ordering remain unchanged.

## Supported Construction Facts

The first official slice recognizes these `ThemeData` constructor families when the file imports
`package:flutter/material.dart`:

- `ThemeData(...)`;
- `ThemeData.light(...)`;
- `ThemeData.dark(...)`;
- `ThemeData.from(...)`.

For those constructors DartScope retains the exact invocation span and the raw expressions supplied
to `brightness`, `colorScheme`, `colorSchemeSeed`, and `useMaterial3`. It does not evaluate those
expressions or infer defaults beyond the normalized constructor kind.

## Supported Application Facts

The first official slice records:

- `MaterialApp.theme`;
- `MaterialApp.darkTheme`;
- `MaterialApp.highContrastTheme`;
- `MaterialApp.highContrastDarkTheme`;
- `MaterialApp.themeMode`;
- `Theme.data`;
- `AnimatedTheme.data`.

Each fact preserves the original expression, exact named-argument span, source path, and high
confidence for the exact official pattern under a Material import.

## Explicit Limits

DartScope does not currently:

- evaluate `ThemeData.copyWith` chains;
- resolve identifiers to declarations or determine which theme wins at runtime;
- interpret `Theme.of` lookups as construction or application;
- normalize component-specific theme classes;
- infer `Brightness` or `ThemeMode` values from arbitrary expressions;
- treat similarly named application classes as Flutter without the official Material import.

These limits keep the output evidence-based and parser-independent.

## Official References

- Flutter `ThemeData`: https://api.flutter.dev/flutter/material/ThemeData-class.html
- Flutter `ThemeData` constructor: https://api.flutter.dev/flutter/material/ThemeData/ThemeData.html
- Flutter `ThemeData.from`: https://api.flutter.dev/flutter/material/ThemeData/ThemeData.from.html
- Flutter `MaterialApp.theme`: https://api.flutter.dev/flutter/material/MaterialApp/theme.html
- Flutter `MaterialApp.darkTheme`: https://api.flutter.dev/flutter/material/MaterialApp/darkTheme.html
- Flutter `MaterialApp.highContrastDarkTheme`:
  https://api.flutter.dev/flutter/material/MaterialApp/highContrastDarkTheme.html
- Flutter `MaterialApp.themeMode`:
  https://api.flutter.dev/flutter/material/MaterialApp/themeMode.html
- Flutter `Theme`: https://api.flutter.dev/flutter/material/Theme-class.html
- Flutter `AnimatedTheme.data`: https://api.flutter.dev/flutter/material/AnimatedTheme/data.html
