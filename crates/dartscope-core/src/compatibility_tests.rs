//! Older JSON documents keep deserializing: additive fields are optional.

use super::*;

#[test]
fn older_flutter_asset_hints_deserialize_without_package_metadata() {
    let hint: FlutterAssetHint = serde_json::from_str(
        r#"{
            "path": "assets/logo.png",
            "source": "image_asset",
            "confidence": "high",
            "span": {
                "byte_start": 0,
                "byte_end": 10,
                "start_line": 1,
                "start_column": 1,
                "end_line": 1,
                "end_column": 11
            }
        }"#,
    )
    .expect("legacy Flutter asset hint");

    assert_eq!(hint.package, None);
    assert_eq!(hint.package_expression, None);
}

#[test]
fn older_diagnostics_deserialize_without_confidence() {
    let diagnostic: DartDiagnostic = serde_json::from_str(
        r#"{"path":"lib/main.dart","code":"example","severity":"warning","message":"example","span":null}"#,
    )
    .expect("legacy diagnostic");

    assert_eq!(diagnostic.confidence, None);
    assert!(
        !serde_json::to_string(&diagnostic)
            .expect("serialize diagnostic")
            .contains("confidence")
    );
}
