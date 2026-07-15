use dartscope_core::pubspec::PubspecDependencySource;
use dartscope_core::{PubspecDependency, PubspecInput};
use dartscope_parse::parse_pubspec;

#[test]
fn stores_typed_and_legacy_dependency_sources_together() {
    let analysis = parse_pubspec(PubspecInput::new(
        "pubspec.yaml",
        concat!(
            "name: demo\n",
            "dependencies:\n",
            "  remote_package:\n",
            "    git:\n",
            "      url: https://example.com/repo.git\n",
            "      ref: stable\n",
            "    version: ^1.0.0\n",
        ),
    ));
    let dependency = &analysis.dependencies[0];

    assert_eq!(
        dependency.source,
        Some(PubspecDependencySource::Git {
            url: Some("https://example.com/repo.git".to_string()),
            reference: Some("stable".to_string()),
            path: None,
            version: Some("^1.0.0".to_string()),
            additional_fields: Vec::new(),
        })
    );
    assert_eq!(
        dependency.version_or_source.as_deref(),
        Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0")
    );

    let json = serde_json::to_value(dependency).expect("serialize dependency");
    assert_eq!(json["source"]["kind"], "git");
    assert_eq!(
        json["version_or_source"],
        "git:ref=stable;url=https://example.com/repo.git;version=^1.0.0"
    );
}

#[test]
fn derives_a_typed_source_from_legacy_json() {
    let dependency: PubspecDependency = serde_json::from_value(serde_json::json!({
        "name": "flutter",
        "section": "dependencies",
        "version_or_source": "sdk:flutter",
        "span": {
            "byte_start": 0,
            "byte_end": 7,
            "start_line": 1,
            "start_column": 1,
            "end_line": 1,
            "end_column": 8
        }
    }))
    .expect("deserialize legacy dependency");

    assert!(dependency.source.is_none());
    assert_eq!(
        dependency.structured_source(),
        Some(PubspecDependencySource::Sdk {
            sdk: "flutter".to_string(),
        })
    );
}
