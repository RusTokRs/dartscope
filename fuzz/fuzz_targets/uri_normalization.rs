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
    let _ = resolve_package_uri(&config, input.as_ref());
});
