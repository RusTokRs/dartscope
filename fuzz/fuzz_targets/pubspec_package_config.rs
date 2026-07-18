#![no_main]

use dartscope_core::{PackageConfigInput, PubspecInput};
use dartscope_parse::parse_pubspec;
use dartscope_resolve::parse_package_config;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = parse_pubspec(PubspecInput::new("pubspec.yaml", source.as_ref()));
    let _ = parse_package_config(PackageConfigInput::new(
        ".dart_tool/package_config.json",
        source.as_ref(),
    ));
});
