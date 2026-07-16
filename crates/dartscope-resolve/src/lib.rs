use std::collections::HashSet;

use dartscope_core::{
    DartDiagnostic, DartPackageConfigEntry, DartResolvedPackageUri, DiagnosticSeverity,
    PackageConfigAnalysis, PackageConfigInput, normalize_path,
};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::Deserialize;
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
    generator: Option<String>,
    #[serde(default)]
    generator_version: Option<String>,
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
    analysis.generator = raw.generator;
    analysis.generator_version = raw.generator_version;
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
  "generator": "pub",
  "generatorVersion": "3.5.0",
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
        assert_eq!(analysis.generator.as_deref(), Some("pub"));
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
