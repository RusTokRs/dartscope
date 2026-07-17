---
id: doc://docs/development/flutter-ecosystem-conventions.md
kind: development_note
language: en
source_language: en
status: active
---

# Versioned Flutter Ecosystem Conventions

DartScope keeps non-SDK Flutter conventions behind an explicit, versioned opt-in API. Package
semantics are activated only when all three evidence classes are present:

1. the caller explicitly enables the convention;
2. a project pubspec declares the package with a supported version constraint;
3. the analyzed file imports that exact package.

This boundary prevents a similarly named class or a future unsupported package major from silently
changing analysis output.

## Public API

- `flutter_ecosystem_support_table()` returns deterministic support metadata.
- `analyze_flutter_ecosystem(&DartProjectAnalysis, enabled)` applies only the listed conventions.
- `FlutterEcosystemAnalysis` retains package evidence, activation status, findings, confidence, and
  exact source spans.
- `FlutterEcosystemSupportTableVersion::V1` is the first serialized policy version.

Passing an empty `enabled` slice produces no convention analyses. The legacy `GoRoute` projection in
`derive_flutter_file_hints` remains available for pre-1.0 compatibility, but new consumers should use
the version-gated API.

## Support Table V1

| Convention | Package | Supported range | Fixture-review version | Initial normalized patterns |
| --- | --- | --- | --- | --- |
| Go Router | `go_router` | `>=14.0.0 <18.0.0` | `17.3.0` | `GoRouter`, `GoRoute`, `ShellRoute`, `StatefulShellRoute`, `StatefulShellBranch` |
| Provider | `provider` | `>=6.0.0 <7.0.0` | `6.1.5+1` | provider widgets plus `BuildContext.watch/read/select` |
| Riverpod | `flutter_riverpod` | `>=2.0.0 <4.0.0` | `3.3.2` | `ProviderScope`, `Consumer`, and consumer base classes |
| BLoC | `flutter_bloc` | `>=8.0.0 <10.0.0` | `9.1.1` | bloc/repository provider, builder, listener, consumer, and selector widgets |

The supported ranges are DartScope fixture contracts. They are intentionally narrower than every
historical release and do not imply complete package API coverage.

## Activation Status

Each enabled convention reports one of:

- `active`: at least one pubspec dependency constraint intersects the supported majors;
- `dependency_missing`: no matching dependency declaration exists;
- `unsupported_version`: all readable constraints are outside the supported majors;
- `unverifiable_version`: only path, git, SDK, workspace, `any`, union, or otherwise unsupported
  constraint evidence is available.

Only `active` entries emit findings. All declarations remain available as package evidence even when
semantics are not activated.

## Constraint Handling

V1 recognizes exact versions, caret constraints, and whitespace-separated comparison conjunctions
such as `>=16.0.0 <18.0.0`. Build metadata is ignored for major-range matching. Complex unions and
opaque source forms remain evidence but are not guessed.

## Confidence And Limits

Ecosystem findings use medium confidence because the conservative parser can prove exact imports,
invocation targets, base-class text, and spans, but it does not perform Dart type resolution. In
particular, DartScope does not currently:

- prove that a locally shadowed identifier comes from the imported package;
- evaluate router trees or provider dependency graphs;
- infer generic types or runtime state ownership;
- activate conventions from transitive dependencies;
- rewrite legacy Flutter inventory JSON.

These limits keep the feature deterministic, additive, and removable with the optional Flutter
crate/feature boundary.

## Package References

- Go Router: https://pub.dev/packages/go_router
- Provider: https://pub.dev/packages/provider
- Flutter Riverpod: https://pub.dev/packages/flutter_riverpod
- Flutter BLoC: https://pub.dev/packages/flutter_bloc
