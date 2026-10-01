# dartscope-resolve

Package configuration v2 parsing and `package:` URI resolution, with no filesystem access.

Part of [DartScope](https://github.com/RusTokRs/dartscope), a standalone Rust toolkit for Dart and Flutter code intelligence. The workspace is pre-1.0: public types and functions may still change between `0.x` releases; the JSON contracts are versioned separately.

## What it is

- `parse_package_config` reads the text of `.dart_tool/package_config.json` into a normalized analysis and
  reports malformed or unsupported content as diagnostics.
- `resolve_package_uri` maps `package:name/path` to a project-relative file the way the Dart tooling does:
  `rootUri` is resolved against the configuration file, `packageUri` against the root, and a path that
  would leave its package directory is rejected.
- URIs are handled by a small in-crate RFC 3986 implementation (syntax check and reference resolution); the
  crate has no URI dependency.

## Example

```rust
use dartscope_core::PackageConfigInput;
use dartscope_resolve::{parse_package_config, resolve_package_uri};

let config = parse_package_config(PackageConfigInput::new(
    ".dart_tool/package_config.json",
    r#"{"configVersion":2,"packages":[{"name":"app","rootUri":"../","packageUri":"lib/"}]}"#,
));
let resolved = resolve_package_uri(&config, "package:app/main.dart");
println!("{resolved:?}");
```

## Documentation

- [API reference](https://docs.rs/dartscope-resolve)
- [Package configuration](https://github.com/RusTokRs/dartscope/blob/main/docs/development/package-config.md)
- [Repository README](https://github.com/RusTokRs/dartscope#readme)

## License

MIT.
