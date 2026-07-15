use dartscope_core::pubspec::{
    PubspecDependencySource, PubspecDependencySourceField,
};

#[test]
fn serializes_every_pubspec_dependency_source_variant() {
    let sources = vec![
        PubspecDependencySource::Version {
            constraint: "^1.2.0".to_string(),
        },
        PubspecDependencySource::Sdk {
            sdk: "flutter".to_string(),
        },
        PubspecDependencySource::Path {
            path: "../local_package".to_string(),
        },
        PubspecDependencySource::Git {
            url: Some("https://example.com/repo.git".to_string()),
            reference: Some("stable".to_string()),
            path: Some("packages/demo".to_string()),
            version: Some("^1.0.0".to_string()),
            additional_fields: vec![PubspecDependencySourceField {
                key: "custom".to_string(),
                value: "value".to_string(),
            }],
        },
        PubspecDependencySource::Hosted {
            name: Some("hosted_package".to_string()),
            url: Some("https://pub.example.com".to_string()),
            version: Some("^2.0.0".to_string()),
            additional_fields: Vec::new(),
        },
        PubspecDependencySource::Workspace,
        PubspecDependencySource::Other {
            value: "custom=value".to_string(),
        },
    ];

    let actual = serde_json::to_value(&sources).expect("serialize dependency sources");
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/pubspec_dependency_sources.json"
    ))
    .expect("parse dependency source fixture");

    assert_eq!(actual, expected);
    let round_trip: Vec<PubspecDependencySource> =
        serde_json::from_value(expected).expect("deserialize dependency sources");
    assert_eq!(round_trip, sources);
}
