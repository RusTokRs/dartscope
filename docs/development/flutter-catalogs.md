# Flutter Asset And Localization Catalogs

DartScope links direct Flutter asset and generated-localization uses without invoking Flutter,
Dart, build scripts, or network services.

## Explicit inputs

The library API accepts supplemental source text through:

- `FlutterL10nInput` for `l10n.yaml`;
- `FlutterArbInput` for ARB JSON files;
- `FlutterCatalogInput` as the deterministic collection passed to
  `extract_flutter_inventory_with_catalogs`.

Input paths must use the same normalized project-relative namespace as the corresponding
`DartProjectAnalysis`. The library performs no filesystem discovery. Only the CLI
`flutter-inventory` command discovers regular `l10n.yaml` and `.arb` files during its project
traversal and passes their text to this API; unrelated project commands do not read them.

## Asset declaration linking

Asset uses are linked to the nearest ancestor `pubspec.yaml` package. A declaration that names a
file matches the same normalized bundle path. A declaration ending in `/` matches files directly in
that directory; it does not recursively cover subdirectories. This follows Flutter's documented
asset-directory behavior.

`Image.asset` and `AssetImage` package arguments are retained. A use naming another package is
external to the nearest local pubspec and is not reported as a local undeclared asset.

Catalog output records declaration kind, selectors, source span, and direct use count. Diagnostics:

- `flutter_asset_used_but_undeclared` — a direct literal local use has no matching declaration;
- `flutter_asset_package_unresolved` — a non-literal `package:` expression prevents exact
  local/external matching;
- `flutter_asset_declared_but_unused` — a declaration has no direct literal use in the analyzed
  package. This is medium-confidence because dynamic construction and unindexed consumers can exist.

## Localization configuration and ARB keys

The supported `l10n.yaml` fields are:

- `arb-dir`, default `lib/l10n`;
- `template-arb-file`, default `app_en.arb`;
- `output-localization-file`, default `app_localizations.dart`;
- `output-class`, default `AppLocalizations`;
- `output-dir`, defaulting to `arb-dir`.

When `flutter.generate: true` is present or a default-directory ARB input exists, DartScope creates
the documented default configuration even when no explicit `l10n.yaml` was supplied.

ARB inputs must be top-level JSON objects. Message keys are top-level properties that do not start
with `@`; `@message` metadata and `@@locale` metadata are not treated as generated localization
members. Referenced getters and placeholder methods are both linked by their ARB key. The
template ARB is authoritative for referenced-key validation.

Diagnostics:

- `flutter_l10n_invalid_yaml`;
- `flutter_l10n_duplicate_configuration`;
- `flutter_l10n_template_missing`;
- `flutter_arb_invalid_json`;
- `flutter_localization_class_unresolved`;
- `flutter_localization_key_missing`.

Catalog diagnostics carry explicit confidence. Exact missing literal keys and configured class
mismatches are high-confidence. Missing supplemental configuration or a default-derived missing
template is medium-confidence because library callers can intentionally provide partial inputs.

## JSON compatibility

The `dartscope.flutter-inventory` v1 model adds optional declaration links, catalog lists, and
catalog diagnostics. Empty additions are omitted from serialized output, so older default fixtures
remain byte-for-byte stable. Existing fields are not removed or renamed.
