use dartscope_core::pubspec::PubspecDependencySource;
use dartscope_core::{PubspecAnalysis, PubspecInput};
use dartscope_parse::{parse_normalized_dependency_source, parse_pubspec};

const SOURCE: &str = r#"
name: demo

dependencies:
  remote_package:
    git:
      url: https://example.com/repo;a=b.git
      ref: stable
  scalar_git:
    git: https://example.com/repo.git
    version: ^1.0.0
  hosted_package:
    hosted:
      name: hosted_package
      url: https://pub.example.com;a=b
  unknown_shape:
    foo:
      bar: x
  plain_package: ^1.25.0
"#;

#[test]
fn preserves_field_separators_inside_dependency_urls() {
    let analysis = parse(SOURCE);

    assert_eq!(
        typed(&analysis, "remote_package"),
        Some(PubspecDependencySource::Git {
            url: Some("https://example.com/repo;a=b.git".to_string()),
            reference: Some("stable".to_string()),
            path: None,
            version: None,
            additional_fields: vec![],
        })
    );
    let legacy = legacy(&analysis, "remote_package").expect("legacy source");
    assert_eq!(
        legacy,
        "git:ref=stable;url=https://example.com/repo\\;a=b.git"
    );
    assert_eq!(
        parse_normalized_dependency_source(&legacy),
        typed(&analysis, "remote_package").expect("typed source")
    );

    assert_eq!(
        typed(&analysis, "hosted_package"),
        Some(PubspecDependencySource::Hosted {
            name: Some("hosted_package".to_string()),
            url: Some("https://pub.example.com;a=b".to_string()),
            version: None,
            additional_fields: vec![],
        })
    );
}

#[test]
fn keeps_scalar_git_urls_with_sibling_versions_distinct() {
    let analysis = parse(SOURCE);

    assert_eq!(
        typed(&analysis, "scalar_git"),
        Some(PubspecDependencySource::Git {
            url: Some("https://example.com/repo.git".to_string()),
            reference: None,
            path: None,
            version: Some("^1.0.0".to_string()),
            additional_fields: vec![],
        })
    );
    let legacy = legacy(&analysis, "scalar_git").expect("legacy source");
    assert_eq!(
        parse_normalized_dependency_source(&legacy),
        typed(&analysis, "scalar_git").expect("typed source")
    );
}

#[test]
fn retains_unknown_nested_dependency_shapes() {
    let analysis = parse(SOURCE);

    assert_eq!(
        typed(&analysis, "unknown_shape"),
        Some(PubspecDependencySource::Other {
            value: "foo.bar=x".to_string(),
        })
    );
}

#[test]
fn keeps_plain_versions_and_canonical_legacy_rendering() {
    let analysis = parse(SOURCE);

    assert_eq!(
        typed(&analysis, "plain_package"),
        Some(PubspecDependencySource::Version {
            constraint: "^1.25.0".to_string(),
        })
    );
    assert_eq!(
        legacy(&analysis, "plain_package").as_deref(),
        Some("^1.25.0")
    );
}

#[test]
fn canonical_sources_without_separators_keep_their_legacy_strings() {
    let analysis = parse(concat!(
        "name: demo\n",
        "dependencies:\n",
        "  remote_package:\n",
        "    git:\n",
        "      url: https://example.com/repo.git\n",
        "      ref: stable\n",
        "    version: ^1.0.0\n",
        "  hosted_package:\n",
        "    hosted:\n",
        "      name: hosted_package\n",
        "      url: https://pub.example.com\n",
        "    version: ^2.0.0\n",
        "  bare_git:\n",
        "    git: https://example.com/bare.git\n",
    ));

    assert_eq!(
        legacy(&analysis, "remote_package").as_deref(),
        Some("git:ref=stable;url=https://example.com/repo.git;version=^1.0.0")
    );
    assert_eq!(
        legacy(&analysis, "hosted_package").as_deref(),
        Some("hosted:name=hosted_package;url=https://pub.example.com;version=^2.0.0")
    );
    assert_eq!(
        legacy(&analysis, "bare_git").as_deref(),
        Some("git:https://example.com/bare.git")
    );
}

fn parse(source: &str) -> PubspecAnalysis {
    parse_pubspec(PubspecInput::new("pubspec.yaml", source))
}

fn typed(analysis: &PubspecAnalysis, name: &str) -> Option<PubspecDependencySource> {
    analysis
        .dependencies
        .iter()
        .find(|dependency| dependency.name == name)
        .and_then(|dependency| dependency.structured_source())
}

fn legacy(analysis: &PubspecAnalysis, name: &str) -> Option<String> {
    analysis
        .dependencies
        .iter()
        .find(|dependency| dependency.name == name)
        .and_then(|dependency| dependency.version_or_source.clone())
}
