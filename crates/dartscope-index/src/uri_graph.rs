use std::collections::{HashMap, HashSet};
use std::path::Path;

use dartscope_core::{
    DartCompilationEnvironment, DartProjectAnalysis, DartUriGraph, DartUriReference,
    DartUriReferenceKind, DartUriResolution, SourceSpan,
};
use dartscope_resolve::{
    PackageUriResolutionError, resolve_package_uri as resolve_configured_package_uri,
};

use crate::paths::{has_uri_scheme, normalize_joined_path, parent_path};

struct UriResolutionContext<'a> {
    known_files: HashSet<&'a str>,
    package_roots: HashMap<&'a str, Vec<String>>,
    package_configs: &'a [dartscope_core::PackageConfigAnalysis],
}

#[derive(Debug, Clone, Default)]
pub struct DartIndexOptions {
    pub compilation_environment: Option<DartCompilationEnvironment>,
}

impl DartIndexOptions {
    pub fn with_compilation_environment(
        mut self,
        compilation_environment: DartCompilationEnvironment,
    ) -> Self {
        self.compilation_environment = Some(compilation_environment);
        self
    }
}

pub fn build_uri_graph(project: &DartProjectAnalysis) -> DartUriGraph {
    build_uri_graph_with_options(project, &DartIndexOptions::default())
}

pub fn build_uri_graph_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartUriGraph {
    let known_files: HashSet<_> = project
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut package_roots: HashMap<&str, Vec<String>> = HashMap::new();

    for pubspec in &project.pubspecs {
        if let Some(package_name) = pubspec.package_name.as_deref() {
            package_roots
                .entry(package_name)
                .or_default()
                .push(parent_path(&pubspec.path));
        }
    }
    for roots in package_roots.values_mut() {
        roots.sort();
        roots.dedup();
    }
    let context = UriResolutionContext {
        known_files,
        package_roots,
        package_configs: &project.package_configs,
    };

    let mut graph = DartUriGraph::default();
    for file in &project.files {
        for import in &file.imports {
            for (uri, condition) in configurable_uris(
                &import.uri,
                &import.configurations,
                options.compilation_environment.as_ref(),
            ) {
                graph.references.push(resolve_uri_reference(
                    &file.path,
                    uri,
                    condition,
                    &import.span,
                    DartUriReferenceKind::Import,
                    &context,
                ));
            }
        }
        for export in &file.exports {
            for (uri, condition) in configurable_uris(
                &export.uri,
                &export.configurations,
                options.compilation_environment.as_ref(),
            ) {
                graph.references.push(resolve_uri_reference(
                    &file.path,
                    uri,
                    condition,
                    &export.span,
                    DartUriReferenceKind::Export,
                    &context,
                ));
            }
        }
        for part in &file.parts {
            graph.references.push(resolve_uri_reference(
                &file.path,
                &part.uri,
                None,
                &part.span,
                DartUriReferenceKind::Part,
                &context,
            ));
        }
    }

    graph.references.sort_by(|left, right| {
        (
            &left.source_path,
            left.source_span.byte_start,
            reference_kind_order(left.kind),
            &left.uri,
        )
            .cmp(&(
                &right.source_path,
                right.source_span.byte_start,
                reference_kind_order(right.kind),
                &right.uri,
            ))
    });
    graph
}

fn configurable_uris<'a>(
    default_uri: &'a str,
    configurations: &'a [dartscope_core::DartUriConfiguration],
    environment: Option<&DartCompilationEnvironment>,
) -> Vec<(&'a str, Option<&'a str>)> {
    let Some(environment) = environment else {
        let mut uris = vec![(default_uri, None)];
        uris.extend(configurations.iter().map(|configuration| {
            (
                configuration.uri.as_str(),
                Some(configuration.condition.as_str()),
            )
        }));
        return uris;
    };

    if let Some(configuration) = configurations
        .iter()
        .find(|configuration| uri_condition_matches(&configuration.condition, environment))
    {
        vec![(
            configuration.uri.as_str(),
            Some(configuration.condition.as_str()),
        )]
    } else {
        vec![(default_uri, None)]
    }
}

fn uri_condition_matches(condition: &str, environment: &DartCompilationEnvironment) -> bool {
    let Some((key, expected)) = parse_uri_condition(condition) else {
        return false;
    };
    environment.get(&key) == Some(expected.as_str())
}

fn parse_uri_condition(condition: &str) -> Option<(String, String)> {
    if let Some((key, value)) = condition.split_once("==") {
        let key = normalize_condition_key(key)?;
        let value = parse_condition_string_literal(value.trim())?;
        Some((key, value))
    } else {
        normalize_condition_key(condition).map(|key| (key, "true".to_string()))
    }
}

fn normalize_condition_key(key: &str) -> Option<String> {
    let normalized = key.split('.').map(str::trim).collect::<Vec<_>>().join(".");
    is_dotted_identifier_list(&normalized).then_some(normalized)
}

fn is_dotted_identifier_list(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut chars = segment.chars();
            chars
                .next()
                .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
}

fn parse_condition_string_literal(value: &str) -> Option<String> {
    let mut chars = value.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut result = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            result.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(result);
        } else {
            result.push(ch);
        }
    }
    None
}

fn resolve_uri_reference(
    source_path: &str,
    uri: &str,
    condition: Option<&str>,
    source_span: &SourceSpan,
    kind: DartUriReferenceKind,
    context: &UriResolutionContext<'_>,
) -> DartUriReference {
    let (resolution, target_path, target_uri, candidate_paths) = if uri.starts_with("dart:") {
        (DartUriResolution::External, None, None, Vec::new())
    } else if let Some(package_uri) = uri.strip_prefix("package:") {
        if let Some(config) = nearest_package_config(source_path, context.package_configs) {
            resolve_package_uri_from_config(config, uri, &context.known_files)
        } else {
            resolve_package_uri_from_pubspecs(
                package_uri,
                &context.known_files,
                &context.package_roots,
            )
        }
    } else if has_uri_scheme(uri) {
        (DartUriResolution::UnsupportedScheme, None, None, Vec::new())
    } else {
        let target = normalize_joined_path(&parent_path(source_path), uri);
        resolution_for_target(target, &context.known_files)
    };

    DartUriReference {
        source_path: source_path.to_string(),
        source_span: source_span.clone(),
        uri: uri.to_string(),
        condition: condition.map(str::to_string),
        kind,
        resolution,
        target_path,
        target_uri,
        candidate_paths,
    }
}

fn resolve_package_uri_from_pubspecs(
    package_uri: &str,
    known_files: &HashSet<&str>,
    package_roots: &HashMap<&str, Vec<String>>,
) -> (
    DartUriResolution,
    Option<String>,
    Option<String>,
    Vec<String>,
) {
    let Some((package_name, library_path)) = package_uri.split_once('/') else {
        return (DartUriResolution::UnindexedPackage, None, None, Vec::new());
    };
    let Some(roots) = package_roots.get(package_name) else {
        return (DartUriResolution::UnindexedPackage, None, None, Vec::new());
    };

    let mut candidates: Vec<_> = roots
        .iter()
        .map(|root| normalize_joined_path(&normalize_joined_path(root, "lib"), library_path))
        .collect();
    candidates.sort();
    candidates.dedup();

    match candidates.as_slice() {
        [target] => resolution_for_target(target.clone(), known_files),
        _ => (DartUriResolution::AmbiguousPackage, None, None, candidates),
    }
}

fn nearest_package_config<'a>(
    source_path: &str,
    package_configs: &'a [dartscope_core::PackageConfigAnalysis],
) -> Option<&'a dartscope_core::PackageConfigAnalysis> {
    package_configs
        .iter()
        .filter_map(|config| package_config_scope(&config.path).map(|scope| (scope, config)))
        .filter(|(scope, _)| path_is_inside_scope(source_path, scope))
        .max_by_key(|(scope, _)| scope.split('/').filter(|part| !part.is_empty()).count())
        .map(|(_, config)| config)
}

fn package_config_scope(path: &str) -> Option<String> {
    let config_directory = parent_path(path);
    (Path::new(&config_directory)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(".dart_tool"))
    .then(|| parent_path(&config_directory))
}

fn path_is_inside_scope(path: &str, scope: &str) -> bool {
    scope.is_empty()
        || path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn resolve_package_uri_from_config(
    config: &dartscope_core::PackageConfigAnalysis,
    package_uri: &str,
    known_files: &HashSet<&str>,
) -> (
    DartUriResolution,
    Option<String>,
    Option<String>,
    Vec<String>,
) {
    match resolve_configured_package_uri(config, package_uri) {
        Ok(resolved) => match resolved.project_path {
            Some(target) => {
                let (resolution, target_path, _, candidate_paths) =
                    resolution_for_target(target, known_files);
                (
                    resolution,
                    target_path,
                    Some(resolved.resolved_uri),
                    candidate_paths,
                )
            }
            None => (
                DartUriResolution::ResolvedExternal,
                None,
                Some(resolved.resolved_uri),
                Vec::new(),
            ),
        },
        Err(PackageUriResolutionError::UnknownPackage(_)) => {
            (DartUriResolution::UnindexedPackage, None, None, Vec::new())
        }
        Err(PackageUriResolutionError::InvalidPackageUri(_)) => {
            (DartUriResolution::InvalidUri, None, None, Vec::new())
        }
        Err(
            PackageUriResolutionError::InvalidConfiguration
            | PackageUriResolutionError::InvalidConfiguredUri(_),
        ) => (
            DartUriResolution::InvalidConfiguration,
            None,
            None,
            Vec::new(),
        ),
    }
}

fn resolution_for_target(
    target: String,
    known_files: &HashSet<&str>,
) -> (
    DartUriResolution,
    Option<String>,
    Option<String>,
    Vec<String>,
) {
    let resolution = if known_files.contains(target.as_str()) {
        DartUriResolution::Resolved
    } else {
        DartUriResolution::MissingTarget
    };
    (resolution, Some(target), None, Vec::new())
}

fn reference_kind_order(kind: DartUriReferenceKind) -> u8 {
    match kind {
        DartUriReferenceKind::Import => 0,
        DartUriReferenceKind::Export => 1,
        DartUriReferenceKind::Part => 2,
    }
}
