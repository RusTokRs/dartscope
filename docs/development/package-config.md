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

URIs are read and resolved by the RFC 3986 module of `dartscope-resolve` (`uri.rs`): the syntax check,
the reference resolution of section 5.2, printing. Three rules make the result safe to use as a path:

- An escaped dot (`%2e`, `%2E`) is a dot, so `%2e%2e/` climbs exactly like `../` (RFC 3986, section 2.3).
- A resolution whose path would start with `//` although the URI has no authority is an error
  (`InvalidConfiguredUri` for a `rootUri` or `packageUri`, `InvalidPackageUri` for the `package:` path):
  printed, such a URI would name a host, not a path.
- `project_path` is a plain relative path or `None`. An escaped separator (`%2f`, `%5c`) in a `rootUri`
  decodes after the dot segments have been removed, so the decoded path is checked again: a `.` or `..`
  segment, a leading `/` or an empty segment (a trailing `/` is allowed), a drive (`C:`) and a control
  character all make it `None`. `resolved_uri` is still returned.

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
