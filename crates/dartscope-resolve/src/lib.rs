use std::collections::HashSet;

use dartscope_core::{
    DartDiagnostic, DartPackageConfigEntry, DartResolvedPackageUri, DiagnosticSeverity,
    PackageConfigAnalysis, PackageConfigInput, normalize_path,
};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use uriparse::{URI, URIReference};

const SUPPORTED_CONFIG_VERSION: u64 = 2;
const PROJECT_URI_ROOT: &str = "file:///__dartscope_project__/";

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum PackageUriResolutionError {
    #[error("package configuration is invalid")]
    InvalidConfiguration,
    #[error("invalid package URI: {0}")]
    InvalidPackageUri(String),
    #[error("package is not present in the configuration: {0}")]
    UnknownPackage(String),
    #[error("invalid URI in package configuration for {0}")]
    InvalidConfiguredUri(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPackageConfig {
    config_version: u64,
    packages: Vec<RawPackageEntry>,
    #[serde(default)]
    generated: Option<Value>,
    #[serde(default)]
    generator: Option<Value>,
    #[serde(default)]
    generator_version: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPackageEntry {
    name: String,
    root_uri: String,
    #[serde(default)]
    package_uri: Option<String>,
    #[serde(default)]
    language_version: Option<String>,
}

pub fn parse_package_config(input: PackageConfigInput) -> PackageConfigAnalysis {
    let mut analysis = PackageConfigAnalysis {
        path: input.path,
        config_version: None,
        packages: Vec::new(),
        generated: None,
        generator: None,
        generator_version: None,
        diagnostics: Vec::new(),
    };

    let raw: RawPackageConfig = match serde_json::from_str(&input.source) {
        Ok(raw) => raw,
        Err(error) => {
            analysis.diagnostics.push(
                DartDiagnostic::error(
                    "package_config_invalid_json",
                    format!("invalid package configuration JSON: {error}"),
                    None,
                )
                .with_path(analysis.path.clone()),
            );
            return analysis;
        }
    };

    analysis.config_version = Some(raw.config_version);
    analysis.generated = parse_optional_metadata(
        raw.generated,
        "generated",
        "package_config_invalid_generated",
        is_generated_timestamp,
        "must use the UTC format YYYY-MM-DDTHH:mm:ss.sssZ",
        &mut analysis.diagnostics,
    );
    analysis.generator = parse_optional_metadata(
        raw.generator,
        "generator",
        "package_config_invalid_generator",
        |_| true,
        "must be a string",
        &mut analysis.diagnostics,
    );
    analysis.generator_version = parse_optional_metadata(
        raw.generator_version,
        "generatorVersion",
        "package_config_invalid_generator_version",
        is_semantic_version,
        "must be a Semantic Version",
        &mut analysis.diagnostics,
    );
    if raw.config_version != SUPPORTED_CONFIG_VERSION {
        analysis.diagnostics.push(DartDiagnostic::error(
            "package_config_unsupported_version",
            format!(
                "unsupported package configuration version {}; expected {}",
                raw.config_version, SUPPORTED_CONFIG_VERSION
            ),
            None,
        ));
    }

    let mut names = HashSet::new();
    for package in raw.packages {
        validate_package_entry(&package, &mut names, &mut analysis.diagnostics);
        analysis.packages.push(DartPackageConfigEntry {
            name: package.name,
            root_uri: package.root_uri,
            package_uri: package.package_uri,
            language_version: package.language_version,
        });
    }

    for diagnostic in &mut analysis.diagnostics {
        if diagnostic.path.is_none() {
            diagnostic.path = Some(analysis.path.clone());
        }
    }
    analysis
}

pub fn resolve_package_uri(
    config: &PackageConfigAnalysis,
    package_uri: &str,
) -> Result<DartResolvedPackageUri, PackageUriResolutionError> {
    if config.config_version != Some(SUPPORTED_CONFIG_VERSION)
        || config
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(PackageUriResolutionError::InvalidConfiguration);
    }

    let Some(package_reference) = package_uri.strip_prefix("package:") else {
        return Err(PackageUriResolutionError::InvalidPackageUri(
            package_uri.to_string(),
        ));
    };
    let Some((package_name, library_path)) = package_reference.split_once('/') else {
        return Err(PackageUriResolutionError::InvalidPackageUri(
            package_uri.to_string(),
        ));
    };
    if !is_package_name(package_name) || !is_relative_uri_path_inside_root(library_path) {
        return Err(PackageUriResolutionError::InvalidPackageUri(
            package_uri.to_string(),
        ));
    }

    let package = config
        .packages
        .iter()
        .find(|package| package.name == package_name)
        .ok_or_else(|| PackageUriResolutionError::UnknownPackage(package_name.to_string()))?;
    let config_uri = project_file_uri(&config.path)?;
    let root_reference = URIReference::try_from(package.root_uri.as_str())
        .map_err(|_| PackageUriResolutionError::InvalidConfiguredUri(package.name.clone()))?;
    let root_uri = directory_uri(config_uri.resolve(&root_reference), &package.name)?;
    let package_base_uri = if let Some(package_uri) = package.package_uri.as_deref() {
        let reference = URIReference::try_from(package_uri)
            .map_err(|_| PackageUriResolutionError::InvalidConfiguredUri(package.name.clone()))?;
        directory_uri(root_uri.resolve(&reference), &package.name)?
    } else {
        root_uri
    };
    let library_reference = URIReference::try_from(library_path)
        .map_err(|_| PackageUriResolutionError::InvalidPackageUri(package_uri.to_string()))?;
    let resolved_uri = package_base_uri.resolve(&library_reference).to_string();

    Ok(DartResolvedPackageUri {
        package_name: package_name.to_string(),
        project_path: project_path_from_uri(&resolved_uri),
        resolved_uri,
    })
}

fn project_file_uri(path: &str) -> Result<URI<'static>, PackageUriResolutionError> {
    let encoded_path = path
        .split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/");
    let value = format!("{PROJECT_URI_ROOT}{encoded_path}");
    URI::try_from(value.as_str())
        .map(URI::into_owned)
        .map_err(|_| {
            PackageUriResolutionError::InvalidConfiguredUri("package config path".to_string())
        })
}

fn directory_uri(
    uri: URI<'_>,
    package_name: &str,
) -> Result<URI<'static>, PackageUriResolutionError> {
    let mut value = uri.to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    URI::try_from(value.as_str())
        .map(URI::into_owned)
        .map_err(|_| PackageUriResolutionError::InvalidConfiguredUri(package_name.to_string()))
}

fn project_path_from_uri(uri: &str) -> Option<String> {
    let encoded = uri.strip_prefix(PROJECT_URI_ROOT)?;
    let decoded = percent_decode_str(encoded).decode_utf8().ok()?;
    Some(normalize_path(decoded.into_owned()))
}

fn parse_optional_metadata(
    value: Option<Value>,
    field: &str,
    code: &str,
    validate: impl FnOnce(&str) -> bool,
    expectation: &str,
    diagnostics: &mut Vec<DartDiagnostic>,
) -> Option<String> {
    let value = value?;
    let Some(value) = value.as_str() else {
        diagnostics.push(DartDiagnostic::warning(
            code,
            format!("{field} {expectation}"),
            None,
        ));
        return None;
    };
    if !validate(value) {
        diagnostics.push(DartDiagnostic::warning(
            code,
            format!("{field} {expectation}"),
            None,
        ));
        return None;
    }
    Some(value.to_string())
}

fn is_generated_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 24
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
        || bytes[23] != b'Z'
    {
        return false;
    }
    let digits = [
        (0, 4),
        (5, 7),
        (8, 10),
        (11, 13),
        (14, 16),
        (17, 19),
        (20, 23),
    ];
    if digits
        .iter()
        .any(|&(start, end)| !bytes[start..end].iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    let number = |start: usize, end: usize| -> u32 {
        value[start..end]
            .parse()
            .expect("validated decimal timestamp field")
    };
    let year = number(0, 4);
    let month = number(5, 7);
    let day = number(8, 10);
    let hour = number(11, 13);
    let minute = number(14, 16);
    let second = number(17, 19);
    year > 0
        && (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

fn is_semantic_version(value: &str) -> bool {
    let (without_build, build) = match value.split_once('+') {
        Some((base, build)) if !build.contains('+') => (base, Some(build)),
        Some(_) => return false,
        None => (value, None),
    };
    if let Some(build) = build
        && !valid_semver_identifiers(build, true)
    {
        return false;
    }
    let (core, prerelease) = match without_build.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (without_build, None),
    };
    if let Some(prerelease) = prerelease
        && !valid_semver_identifiers(prerelease, false)
    {
        return false;
    }
    let mut core = core.split('.');
    let Some(major) = core.next() else {
        return false;
    };
    let Some(minor) = core.next() else {
        return false;
    };
    let Some(patch) = core.next() else {
        return false;
    };
    core.next().is_none()
        && valid_semver_number(major)
        && valid_semver_number(minor)
        && valid_semver_number(patch)
}

fn valid_semver_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn valid_semver_identifiers(value: &str, allow_leading_zero_numeric: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (allow_leading_zero_numeric
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

fn validate_package_entry(
    package: &RawPackageEntry,
    names: &mut HashSet<String>,
    diagnostics: &mut Vec<DartDiagnostic>,
) {
    if !is_package_name(&package.name) {
        diagnostics.push(DartDiagnostic::error(
            "package_config_invalid_name",
            format!("invalid package name {:?}", package.name),
            None,
        ));
    } else if !names.insert(package.name.clone()) {
        diagnostics.push(DartDiagnostic::error(
            "package_config_duplicate_name",
            format!("duplicate package name {:?}", package.name),
            None,
        ));
    }

    if !is_root_uri(&package.root_uri) {
        diagnostics.push(DartDiagnostic::error(
            "package_config_invalid_root_uri",
            format!(
                "rootUri for package {:?} must not contain a query or fragment",
                package.name
            ),
            None,
        ));
    }
    if let Some(package_uri) = package.package_uri.as_deref()
        && !is_relative_uri_path_inside_root(package_uri)
    {
        diagnostics.push(DartDiagnostic::error(
            "package_config_invalid_package_uri",
            format!(
                "packageUri for package {:?} must stay inside rootUri",
                package.name
            ),
            None,
        ));
    }
    if let Some(version) = package.language_version.as_deref()
        && !is_language_version(version)
    {
        diagnostics.push(DartDiagnostic::error(
            "package_config_invalid_language_version",
            format!(
                "languageVersion for package {:?} must use major.minor",
                package.name
            ),
            None,
        ));
    }
}

fn is_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().any(|ch| ch != '.')
        && name.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '-' | '.'
                        | '_'
                        | '~'
                        | '!'
                        | '$'
                        | '&'
                        | '\''
                        | '('
                        | ')'
                        | '*'
                        | '+'
                        | ','
                        | ';'
                        | '='
                        | '@'
                )
        })
}

fn is_relative_uri_path_inside_root(uri: &str) -> bool {
    let Ok(reference) = URIReference::try_from(uri) else {
        return false;
    };
    if reference.scheme().is_some()
        || reference.authority().is_some()
        || reference.query().is_some()
        || reference.fragment().is_some()
        || uri.starts_with('/')
    {
        return false;
    }
    let mut depth = 0usize;
    for segment in uri.split('/') {
        match segment {
            "" | "." => {}
            ".." if depth == 0 => return false,
            ".." => depth -= 1,
            _ => depth += 1,
        }
    }
    true
}

fn is_root_uri(uri: &str) -> bool {
    URIReference::try_from(uri)
        .is_ok_and(|reference| reference.query().is_none() && reference.fragment().is_none())
}

fn is_language_version(version: &str) -> bool {
    let Some((major, minor)) = version.split_once('.') else {
        return false;
    };
    !major.contains('.') && valid_decimal(major) && valid_decimal(minor)
}

fn valid_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|ch| ch.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::DiagnosticSeverity;

    #[test]
    fn parses_v2_configuration_and_ignores_extension_fields() {
        let analysis = parse_package_config(PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {
      "name": "app",
      "rootUri": "../",
      "packageUri": "lib/",
      "languageVersion": "3.5",
      "extensionData": true
    },
    {
      "name": "dependency",
      "rootUri": "file:///cache/dependency/"
    }
  ],
  "generated": "2026-07-16T04:24:56.123Z",
  "generator": "pub",
  "generatorVersion": "3.5.0-dev.1+build.7",
  "toolMetadata": {"keptByGenerator": true}
}"#,
        ));

        assert_eq!(analysis.config_version, Some(2));
        assert_eq!(analysis.packages.len(), 2);
        assert_eq!(analysis.packages[0].name, "app");
        assert_eq!(analysis.packages[0].package_uri.as_deref(), Some("lib/"));
        assert_eq!(
            analysis.packages[0].language_version.as_deref(),
            Some("3.5")
        );
        assert_eq!(
            analysis.generated.as_deref(),
            Some("2026-07-16T04:24:56.123Z")
        );
        assert_eq!(analysis.generator.as_deref(), Some("pub"));
        assert_eq!(
            analysis.generator_version.as_deref(),
            Some("3.5.0-dev.1+build.7")
        );
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn rejects_invalid_json_and_unsupported_versions() {
        let invalid = parse_package_config(PackageConfigInput::new("config.json", "{"));
        assert_eq!(invalid.diagnostics.len(), 1);
        assert_eq!(invalid.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(invalid.diagnostics[0].code, "package_config_invalid_json");
        assert_eq!(invalid.diagnostics[0].path.as_deref(), Some("config.json"));

        let unsupported = parse_package_config(PackageConfigInput::new(
            "config.json",
            r#"{"configVersion":3,"packages":[]}"#,
        ));
        assert_eq!(unsupported.config_version, Some(3));
        assert_eq!(
            unsupported.diagnostics[0].code,
            "package_config_unsupported_version"
        );
        assert_eq!(
            unsupported.diagnostics[0].path.as_deref(),
            Some("config.json")
        );
    }

    #[test]
    fn diagnoses_invalid_optional_metadata_without_blocking_resolution() {
        let analysis = parse_package_config(PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [{"name":"app","rootUri":"../","packageUri":"lib/"}],
  "generated": "2026-02-29T25:61:61.123Z",
  "generator": 7,
  "generatorVersion": "03.5.0"
}"#,
        ));

        assert_eq!(analysis.generated, None);
        assert_eq!(analysis.generator, None);
        assert_eq!(analysis.generator_version, None);
        assert_eq!(analysis.diagnostics.len(), 3);
        assert!(analysis.diagnostics.iter().all(|diagnostic| {
            diagnostic.severity == DiagnosticSeverity::Warning
                && diagnostic.path.as_deref() == Some(".dart_tool/package_config.json")
        }));
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "package_config_invalid_generated")
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "package_config_invalid_generator")
        );
        assert!(
            analysis.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "package_config_invalid_generator_version"
            })
        );
        assert!(resolve_package_uri(&analysis, "package:app/main.dart").is_ok());
    }

    #[test]
    fn validates_generated_timestamp_and_semantic_version_edges() {
        for valid in ["2019-09-24T12:34:56.000Z", "2024-02-29T23:59:59.999Z"] {
            assert!(is_generated_timestamp(valid), "{valid}");
        }
        for invalid in [
            "2023-02-29T12:34:56.000Z",
            "2024-13-01T12:34:56.000Z",
            "2024-01-01T24:00:00.000Z",
            "2024-01-01T00:00:00Z",
        ] {
            assert!(!is_generated_timestamp(invalid), "{invalid}");
        }
        for valid in ["0.0.0", "3.5.0-dev.0.2", "3.5.0+build.007"] {
            assert!(is_semantic_version(valid), "{valid}");
        }
        for invalid in ["3.5", "03.5.0", "3.05.0", "3.5.0-01", "3.5.0+"] {
            assert!(!is_semantic_version(invalid), "{invalid}");
        }
    }

    #[test]
    fn validates_names_uris_versions_and_duplicates() {
        let analysis = parse_package_config(PackageConfigInput::new(
            "config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"valid","rootUri":"../","packageUri":"../outside/","languageVersion":"03.1"},
    {"name":"valid","rootUri":"../?query"},
    {"name":"..","rootUri":"../"}
  ]
}"#,
        ));
        let codes: Vec<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains(&"package_config_invalid_package_uri"));
        assert!(codes.contains(&"package_config_invalid_language_version"));
        assert!(codes.contains(&"package_config_duplicate_name"));
        assert!(codes.contains(&"package_config_invalid_root_uri"));
        assert!(codes.contains(&"package_config_invalid_name"));
    }

    #[test]
    fn resolves_relative_entries_to_project_paths() {
        let config = parse_package_config(PackageConfigInput::new(
            "apps/demo/.dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"demo","rootUri":"../","packageUri":"lib/"},
    {"name":"shared","rootUri":"../../../packages/shared","packageUri":"lib"}
  ]
}"#,
        ));

        let demo = resolve_package_uri(&config, "package:demo/src/api.dart").unwrap();
        assert_eq!(
            demo.project_path.as_deref(),
            Some("apps/demo/lib/src/api.dart")
        );
        assert_eq!(
            demo.resolved_uri,
            "file:///__dartscope_project__/apps/demo/lib/src/api.dart"
        );

        let shared = resolve_package_uri(&config, "package:shared/shared.dart").unwrap();
        assert_eq!(
            shared.project_path.as_deref(),
            Some("packages/shared/lib/shared.dart")
        );
    }

    #[test]
    fn preserves_external_package_uris_without_claiming_a_project_path() {
        let config = parse_package_config(PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{
  "configVersion": 2,
  "packages": [
    {"name":"graphql","rootUri":"file:///cache/graphql-5.2.0/","packageUri":"lib/"}
  ]
}"#,
        ));

        let resolved = resolve_package_uri(&config, "package:graphql/client.dart").unwrap();
        assert_eq!(
            resolved.resolved_uri,
            "file:///cache/graphql-5.2.0/lib/client.dart"
        );
        assert_eq!(resolved.project_path, None);
    }

    #[test]
    fn refuses_unknown_packages_invalid_uris_and_invalid_configs() {
        let config = parse_package_config(PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{"configVersion":2,"packages":[]}"#,
        ));
        assert_eq!(
            resolve_package_uri(&config, "package:missing/api.dart"),
            Err(PackageUriResolutionError::UnknownPackage(
                "missing".to_string()
            ))
        );
        assert!(matches!(
            resolve_package_uri(&config, "package:missing"),
            Err(PackageUriResolutionError::InvalidPackageUri(_))
        ));

        let invalid = parse_package_config(PackageConfigInput::new(
            ".dart_tool/package_config.json",
            r#"{"configVersion":3,"packages":[]}"#,
        ));
        assert_eq!(
            resolve_package_uri(&invalid, "package:any/api.dart"),
            Err(PackageUriResolutionError::InvalidConfiguration)
        );
    }
}
