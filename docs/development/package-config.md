# Package Configuration Resolution

DartScope implements Dart package configuration version 2 in `dartscope-resolve`.
The public model retains package entries and optional generator metadata while URI parsing,
canonical directory comparison, and containment checks remain private implementation details.

## Validation policy

Parsing preserves valid entries even when diagnostics are produced. Resolution is stricter:
any error diagnostic makes the complete configuration invalid and
`resolve_package_uri` returns `PackageUriResolutionError::InvalidConfiguration`.
Invalid optional `generated`, `generator`, or `generatorVersion` values produce warnings and
are omitted from the normalized model, so they do not block otherwise valid resolution.

Unknown JSON properties are ignored for forward compatibility.

## URI normalization and containment

`rootUri` is resolved relative to the package-config file URI and normalized as a directory.
`packageUri` is resolved relative to its package root. Canonical comparison uses a normalized
scheme and authority plus percent-decoded path segments.

The resolver rejects:

- duplicate package root directories, including percent-escape-equivalent spellings;
- a package URI directory that contains a nested package root;
- a package URI directory that is contained by a nested package root;
- literal or percent-encoded traversal outside the package root;
- percent-encoded slash or backslash separators inside relative package paths.

Nested roots remain valid when the outer package URI directory and nested root are disjoint.
Absolute external and Windows file URIs are preserved, while only URIs under DartScope's
synthetic project root receive a normalized `project_path`.

## Diagnostic codes

- `package_config_duplicate_root`
- `package_config_package_uri_root_overlap`
- `package_config_invalid_package_uri`
- `package_config_invalid_root_uri`
- `package_config_invalid_generated`
- `package_config_invalid_generator`
- `package_config_invalid_generator_version`

Normative behavior follows Dart's package-config v2 specification in
`dart-lang/language/accepted/2.8/language-versioning/package-config-file-v2.md`.
