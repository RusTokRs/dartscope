#![no_main]

use dartscope_core::{PackageConfigInput, normalize_path};
use dartscope_resolve::{parse_package_config, resolve_package_uri};
use libfuzzer_sys::fuzz_target;

const PACKAGE_CONFIG: &str = r#"{
  "configVersion": 2,
  "packages": [
    {
      "name": "example",
      "rootUri": "../",
      "packageUri": "lib/",
      "languageVersion": "3.13"
    }
  ]
}"#;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let normalized = normalize_path(input.to_string());
    assert_eq!(normalize_path(normalized.clone()), normalized);
    assert!(!normalized.contains('\\'));

    let config = parse_package_config(PackageConfigInput::new(
        ".dart_tool/package_config.json",
        PACKAGE_CONFIG,
    ));
    if let Ok(resolved) = resolve_package_uri(&config, input.as_ref()) {
        assert_plain_relative(resolved.project_path.as_deref());
    }

    // The same bytes as the `rootUri` and `packageUri` of a package: a configuration is untrusted input
    // too, and what it resolves to must never be a path that climbs, is absolute or names a drive.
    let escaped = json_string_body(&input);
    let hostile = parse_package_config(PackageConfigInput::new(
        ".dart_tool/package_config.json",
        format!(
            r#"{{"configVersion":2,"packages":[{{"name":"fuzz","rootUri":"{escaped}","packageUri":"{escaped}"}}]}}"#
        ),
    ));
    for package_uri in ["package:fuzz/a.dart", "package:fuzz/", &*input] {
        if let Ok(resolved) = resolve_package_uri(&hostile, package_uri) {
            assert_plain_relative(resolved.project_path.as_deref());
        }
    }
});

/// What a project path must be once decoded: relative, without climbs, empty segments or drives.
fn assert_plain_relative(path: Option<&str>) {
    let Some(path) = path else {
        return;
    };
    assert!(!path.starts_with('/'), "absolute project path {path:?}");
    assert!(!path.contains('\\'), "backslash in project path {path:?}");
    assert!(
        !path.chars().any(char::is_control),
        "control character in project path {path:?}"
    );
    for segment in path.split('/') {
        assert!(
            segment != ".." && segment != ".",
            "dot segment in project path {path:?}"
        );
    }
    let bytes = path.as_bytes();
    assert!(
        !(bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'),
        "drive in project path {path:?}"
    );
}

/// The text escaped so that it can sit between the quotes of a JSON string.
fn json_string_body(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            control if control < ' ' => escaped.push_str(&format!("\\u{:04x}", control as u32)),
            other => escaped.push(other),
        }
    }
    escaped
}
