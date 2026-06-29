use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use dartscope_core::{
    DartCompilationEnvironment, DartEnclosingSymbol, DartGraphqlBindingResolution,
    DartGraphqlCallCompatibility, DartGraphqlClientCall, DartGraphqlContractAnalysis,
    DartGraphqlOperation, DartGraphqlOperationBinding, DartGraphqlOperationType,
    DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason,
    DartGraphqlVariableCompatibility, DartNamespaceCombinatorKind, DartPartLink,
    DartPartLinkAnalysis, DartPartLinkStatus, DartPartOfKind, DartProjectAnalysis, DartUriGraph,
    DartUriReference, DartUriReferenceKind, DartUriResolution, SourceSpan,
};
use dartscope_resolve::{
    resolve_package_uri as resolve_configured_package_uri, PackageUriResolutionError,
};

struct OperationLocation<'a> {
    path: &'a str,
    operation: &'a DartGraphqlOperation,
}

struct ImportedOperationCandidate<'a> {
    location: &'a OperationLocation<'a>,
    basis: DartGraphqlBindingResolution,
}

struct ImportedOperationResolution<'a> {
    candidates: Vec<ImportedOperationCandidate<'a>>,
    conditional_environment_required: bool,
}

#[derive(Default)]
struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
    members_by_owner: HashMap<String, Vec<String>>,
}

struct ExportResolutionContext<'a, 'context> {
    constant_name: &'context str,
    candidates: &'a [OperationLocation<'a>],
    uri_graph: &'context DartUriGraph,
    files_by_path: &'context HashMap<&'a str, &'a dartscope_core::DartFileAnalysis>,
    library_membership: &'context LibraryMembership,
    options: &'context DartIndexOptions,
}

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

impl LibraryMembership {
    fn from_project(project: &DartProjectAnalysis) -> Self {
        let mut membership = Self::default();
        let mut owners_by_part: HashMap<String, Vec<String>> = HashMap::new();
        for link in analyze_part_links(project)
            .links
            .into_iter()
            .filter(|link| link.status == DartPartLinkStatus::Matched)
        {
            let Some(part_path) = link.part_path else {
                continue;
            };
            owners_by_part
                .entry(part_path)
                .or_default()
                .push(link.owner_path);
        }
        for (part_path, mut owners) in owners_by_part {
            owners.sort();
            owners.dedup();
            let [owner_path] = owners.as_slice() else {
                continue;
            };
            membership
                .owner_by_part
                .insert(part_path.clone(), owner_path.clone());
            membership
                .members_by_owner
                .entry(owner_path.clone())
                .or_default()
                .push(part_path);
        }
        for members in membership.members_by_owner.values_mut() {
            members.sort();
            members.dedup();
        }
        membership
    }

    fn owner_of<'a>(&'a self, path: &'a str) -> &'a str {
        self.owner_by_part
            .get(path)
            .map(String::as_str)
            .unwrap_or(path)
    }

    fn is_part(&self, path: &str) -> bool {
        self.owner_by_part.contains_key(path)
    }

    fn same_library(&self, left: &str, right: &str) -> bool {
        self.owner_of(left) == self.owner_of(right)
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

pub fn analyze_part_links(project: &DartProjectAnalysis) -> DartPartLinkAnalysis {
    let uri_graph = build_uri_graph(project);
    let files_by_path: HashMap<_, _> = project
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut links = Vec::new();

    for reference in uri_graph
        .references
        .iter()
        .filter(|reference| reference.kind == DartUriReferenceKind::Part)
    {
        let Some(part_path) = reference.target_path.as_deref() else {
            links.push(part_link_without_target(reference));
            continue;
        };
        let Some(part_file) = files_by_path.get(part_path) else {
            links.push(part_link_without_target(reference));
            continue;
        };
        let Some(part_of) = part_file.part_of.as_ref() else {
            links.push(DartPartLink {
                owner_path: reference.source_path.clone(),
                part_uri: reference.uri.clone(),
                part_path: Some(part_path.to_string()),
                declared_owner: None,
                status: DartPartLinkStatus::MissingPartOf,
                part_span: reference.source_span.clone(),
                part_of_span: None,
            });
            continue;
        };

        let matches_owner = match part_of.kind {
            DartPartOfKind::Uri => {
                normalize_joined_path(&parent_path(part_path), &part_of.library)
                    == reference.source_path
            }
            DartPartOfKind::LibraryName => {
                files_by_path
                    .get(reference.source_path.as_str())
                    .and_then(|owner| owner.library.as_ref())
                    .and_then(|library| library.name.as_deref())
                    == Some(part_of.library.as_str())
            }
        };

        links.push(DartPartLink {
            owner_path: reference.source_path.clone(),
            part_uri: reference.uri.clone(),
            part_path: Some(part_path.to_string()),
            declared_owner: Some(part_of.library.clone()),
            status: if matches_owner {
                DartPartLinkStatus::Matched
            } else {
                DartPartLinkStatus::DifferentLibrary
            },
            part_span: reference.source_span.clone(),
            part_of_span: Some(part_of.span.clone()),
        });
    }

    DartPartLinkAnalysis { links }
}

fn part_link_without_target(reference: &DartUriReference) -> DartPartLink {
    DartPartLink {
        owner_path: reference.source_path.clone(),
        part_uri: reference.uri.clone(),
        part_path: reference.target_path.clone(),
        declared_owner: None,
        status: if reference.resolution == DartUriResolution::MissingTarget {
            DartPartLinkStatus::MissingTarget
        } else {
            DartPartLinkStatus::UnresolvedTarget
        },
        part_span: reference.source_span.clone(),
        part_of_span: None,
    }
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

fn parent_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn normalize_joined_path(base: &str, relative: &str) -> String {
    let path = Path::new(base).join(relative);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

fn has_uri_scheme(uri: &str) -> bool {
    uri.find(':')
        .is_some_and(|colon| !uri[..colon].contains('/'))
}

fn reference_kind_order(kind: DartUriReferenceKind) -> u8 {
    match kind {
        DartUriReferenceKind::Import => 0,
        DartUriReferenceKind::Export => 1,
        DartUriReferenceKind::Part => 2,
    }
}

pub fn analyze_graphql_contracts(project: &DartProjectAnalysis) -> DartGraphqlContractAnalysis {
    analyze_graphql_contracts_with_options(project, &DartIndexOptions::default())
}

pub fn analyze_graphql_contracts_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartGraphqlContractAnalysis {
    let uri_graph = build_uri_graph_with_options(project, options);
    let library_membership = LibraryMembership::from_project(project);
    let files_by_path: HashMap<_, _> = project
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut operations_by_constant: HashMap<&str, Vec<OperationLocation<'_>>> = HashMap::new();

    for file in &project.files {
        for operation in &file.graphql_operations {
            operations_by_constant
                .entry(&operation.constant_name)
                .or_default()
                .push(OperationLocation {
                    path: &file.path,
                    operation,
                });
        }
    }

    let mut analysis = DartGraphqlContractAnalysis::default();

    for file in &project.files {
        for operation_use in &file.graphql_operation_uses {
            let candidates = operations_by_constant
                .get(operation_use.constant_name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let same_file_candidates: Vec<_> = candidates
                .iter()
                .filter(|location| location.path == file.path)
                .collect();
            let same_library_candidates: Vec<_> = candidates
                .iter()
                .filter(|location| {
                    location.path != file.path
                        && library_membership.same_library(location.path, &file.path)
                })
                .collect();
            let namespace_owner = library_membership.owner_of(&file.path);
            let namespace_file = files_by_path.get(namespace_owner).copied().unwrap_or(file);
            let imported_resolution = imported_operation_candidates(
                namespace_file,
                operation_use.constant_name.as_str(),
                candidates,
                &uri_graph,
                &files_by_path,
                &library_membership,
                options,
            );

            match (
                same_file_candidates.as_slice(),
                same_library_candidates.as_slice(),
                imported_resolution.candidates.as_slice(),
            ) {
                ([location], _, _) => analysis.bindings.push(build_binding(
                    location,
                    DartGraphqlBindingResolution::SameFile,
                    &file.path,
                    operation_use.client_call,
                    &operation_use.variable_names,
                    &operation_use.span,
                    operation_use.enclosing_symbol.clone(),
                )),
                ([], [location], _) => analysis.bindings.push(build_binding(
                    location,
                    DartGraphqlBindingResolution::SameLibrary,
                    &file.path,
                    operation_use.client_call,
                    &operation_use.variable_names,
                    &operation_use.span,
                    operation_use.enclosing_symbol.clone(),
                )),
                ([], [], [candidate]) => analysis.bindings.push(build_binding(
                    candidate.location,
                    candidate.basis,
                    &file.path,
                    operation_use.client_call,
                    &operation_use.variable_names,
                    &operation_use.span,
                    operation_use.enclosing_symbol.clone(),
                )),
                ([], [], []) if imported_resolution.conditional_environment_required => {
                    let locations: Vec<_> = candidates.iter().collect();
                    push_unresolved_use(
                        &mut analysis,
                        operation_use,
                        &file.path,
                        DartGraphqlUnresolvedReason::ConditionalEnvironmentRequired,
                        &locations,
                    );
                }
                ([], [], []) if candidates.is_empty() => push_unresolved_use(
                    &mut analysis,
                    operation_use,
                    &file.path,
                    DartGraphqlUnresolvedReason::MissingDeclaration,
                    &[],
                ),
                ([], [], []) => {
                    let locations: Vec<_> = candidates.iter().collect();
                    push_unresolved_use(
                        &mut analysis,
                        operation_use,
                        &file.path,
                        DartGraphqlUnresolvedReason::NotVisibleDeclaration,
                        &locations,
                    );
                }
                (same_file, same_library, imported) => {
                    let locations: Vec<_> = if !same_file.is_empty() {
                        same_file.to_vec()
                    } else if !same_library.is_empty() {
                        same_library.to_vec()
                    } else {
                        imported
                            .iter()
                            .map(|candidate| candidate.location)
                            .collect()
                    };
                    push_unresolved_use(
                        &mut analysis,
                        operation_use,
                        &file.path,
                        DartGraphqlUnresolvedReason::AmbiguousDeclaration,
                        &locations,
                    );
                }
            }
        }
    }

    analysis.bindings.sort_by(|left, right| {
        (
            &left.use_path,
            left.use_span.byte_start,
            &left.constant_name,
        )
            .cmp(&(
                &right.use_path,
                right.use_span.byte_start,
                &right.constant_name,
            ))
    });
    analysis.unresolved_uses.sort_by(|left, right| {
        (
            &left.use_path,
            left.use_span.byte_start,
            &left.constant_name,
        )
            .cmp(&(
                &right.use_path,
                right.use_span.byte_start,
                &right.constant_name,
            ))
    });

    analysis
}

fn push_unresolved_use(
    analysis: &mut DartGraphqlContractAnalysis,
    operation_use: &dartscope_core::DartGraphqlOperationUse,
    use_path: &str,
    reason: DartGraphqlUnresolvedReason,
    locations: &[&OperationLocation<'_>],
) {
    let mut candidate_paths: Vec<_> = locations
        .iter()
        .map(|location| location.path.to_string())
        .collect();
    candidate_paths.sort();
    candidate_paths.dedup();
    analysis
        .unresolved_uses
        .push(DartGraphqlUnresolvedOperationUse {
            constant_name: operation_use.constant_name.clone(),
            reason,
            use_path: use_path.to_string(),
            use_span: operation_use.span.clone(),
            candidate_paths,
        });
}

fn imported_operation_candidates<'a>(
    file: &dartscope_core::DartFileAnalysis,
    constant_name: &str,
    candidates: &'a [OperationLocation<'a>],
    uri_graph: &DartUriGraph,
    files_by_path: &HashMap<&'a str, &'a dartscope_core::DartFileAnalysis>,
    library_membership: &LibraryMembership,
    options: &DartIndexOptions,
) -> ImportedOperationResolution<'a> {
    if constant_name.starts_with('_') {
        return ImportedOperationResolution {
            candidates: Vec::new(),
            conditional_environment_required: false,
        };
    }

    let context = ExportResolutionContext {
        constant_name,
        candidates,
        uri_graph,
        files_by_path,
        library_membership,
        options,
    };
    let mut result = Vec::new();
    let mut conditional_environment_required = false;
    for import in &file.imports {
        if import.prefix.is_some()
            || import.is_deferred
            || !namespace_allows_name(&import.combinators, constant_name)
        {
            continue;
        }
        if !import.configurations.is_empty() && options.compilation_environment.is_none() {
            conditional_environment_required = true;
            continue;
        }
        if let Some(target_path) = resolved_namespace_target(
            uri_graph,
            DartUriReferenceKind::Import,
            &file.path,
            &import.span,
        ) {
            let mut exported = Vec::new();
            collect_exported_operations(
                target_path,
                &context,
                &mut HashSet::new(),
                &mut conditional_environment_required,
                &mut exported,
            );
            result.extend(
                exported
                    .into_iter()
                    .map(|location| ImportedOperationCandidate {
                        basis: if library_membership.owner_of(location.path) == target_path {
                            DartGraphqlBindingResolution::DirectImport
                        } else {
                            DartGraphqlBindingResolution::ReExport
                        },
                        location,
                    }),
            );
        }
    }

    result.sort_by_key(|candidate| {
        (
            candidate.location.path,
            candidate.location.operation.span.byte_start,
            binding_basis_order(candidate.basis),
        )
    });
    result.dedup_by_key(|candidate| {
        (
            candidate.location.path,
            candidate.location.operation.span.byte_start,
        )
    });
    ImportedOperationResolution {
        candidates: result,
        conditional_environment_required,
    }
}

fn collect_exported_operations<'a>(
    library_path: &str,
    context: &ExportResolutionContext<'a, '_>,
    visited: &mut HashSet<String>,
    conditional_environment_required: &mut bool,
    result: &mut Vec<&'a OperationLocation<'a>>,
) {
    if !visited.insert(library_path.to_string()) {
        return;
    }
    if context.library_membership.is_part(library_path) {
        return;
    }

    result.extend(
        context.candidates.iter().filter(|candidate| {
            context.library_membership.owner_of(candidate.path) == library_path
        }),
    );

    let Some(file) = context.files_by_path.get(library_path) else {
        return;
    };
    for export in &file.exports {
        if !namespace_allows_name(&export.combinators, context.constant_name) {
            continue;
        }
        if !export.configurations.is_empty() && context.options.compilation_environment.is_none() {
            *conditional_environment_required = true;
            continue;
        }
        if let Some(target_path) = resolved_namespace_target(
            context.uri_graph,
            DartUriReferenceKind::Export,
            library_path,
            &export.span,
        ) {
            collect_exported_operations(
                target_path,
                context,
                visited,
                conditional_environment_required,
                result,
            );
        }
    }
}

fn resolved_namespace_target<'a>(
    uri_graph: &'a DartUriGraph,
    kind: DartUriReferenceKind,
    source_path: &str,
    span: &SourceSpan,
) -> Option<&'a str> {
    uri_graph.references.iter().find_map(|reference| {
        (reference.kind == kind
            && reference.source_path == source_path
            && reference.source_span.byte_start == span.byte_start
            && reference.resolution == DartUriResolution::Resolved)
            .then_some(reference.target_path.as_deref())
            .flatten()
    })
}

fn binding_basis_order(basis: DartGraphqlBindingResolution) -> u8 {
    match basis {
        DartGraphqlBindingResolution::SameFile => 0,
        DartGraphqlBindingResolution::SameLibrary => 1,
        DartGraphqlBindingResolution::DirectImport => 2,
        DartGraphqlBindingResolution::ReExport => 3,
    }
}

fn namespace_allows_name(
    combinators: &[dartscope_core::DartNamespaceCombinator],
    name: &str,
) -> bool {
    combinators.iter().all(|combinator| match combinator.kind {
        DartNamespaceCombinatorKind::Show => combinator.names.iter().any(|shown| shown == name),
        DartNamespaceCombinatorKind::Hide => combinator.names.iter().all(|hidden| hidden != name),
    })
}

fn build_binding(
    location: &OperationLocation<'_>,
    resolution_basis: DartGraphqlBindingResolution,
    use_path: &str,
    client_call: DartGraphqlClientCall,
    supplied_variable_names: &[String],
    use_span: &SourceSpan,
    enclosing_symbol: Option<DartEnclosingSymbol>,
) -> DartGraphqlOperationBinding {
    let operation = location.operation;
    let missing_variable_names = difference(&operation.variable_names, supplied_variable_names);
    let unexpected_variable_names = difference(supplied_variable_names, &operation.variable_names);
    let variable_compatibility =
        if missing_variable_names.is_empty() && unexpected_variable_names.is_empty() {
            DartGraphqlVariableCompatibility::Match
        } else {
            DartGraphqlVariableCompatibility::Mismatch
        };

    DartGraphqlOperationBinding {
        constant_name: operation.constant_name.clone(),
        resolution_basis,
        operation_name: operation.operation_name.clone(),
        operation_type: operation.operation_type,
        client_call,
        call_compatibility: call_compatibility(operation.operation_type, client_call),
        declared_variable_names: operation.variable_names.clone(),
        supplied_variable_names: supplied_variable_names.to_vec(),
        missing_variable_names,
        unexpected_variable_names,
        variable_compatibility,
        operation_path: location.path.to_string(),
        operation_span: operation.span.clone(),
        use_path: use_path.to_string(),
        use_span: use_span.clone(),
        enclosing_symbol,
    }
}

fn difference(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|name| !right.contains(name))
        .cloned()
        .collect()
}

fn call_compatibility(
    operation_type: DartGraphqlOperationType,
    client_call: DartGraphqlClientCall,
) -> DartGraphqlCallCompatibility {
    match (operation_type, client_call) {
        (_, DartGraphqlClientCall::Unknown) => DartGraphqlCallCompatibility::Unknown,
        (DartGraphqlOperationType::Query, DartGraphqlClientCall::Query)
        | (DartGraphqlOperationType::Mutation, DartGraphqlClientCall::Mutation)
        | (DartGraphqlOperationType::Subscription, DartGraphqlClientCall::Subscription) => {
            DartGraphqlCallCompatibility::Match
        }
        _ => DartGraphqlCallCompatibility::Mismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::{
        DartCompilationEnvironment, DartFileInput, DartProjectInput, PackageConfigInput,
    };
    use dartscope_parse::analyze_project;

    #[test]
    fn resolves_relative_package_sdk_and_missing_uri_references() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "packages/app/lib/app.dart",
                    r#"
import 'dart:async';
import 'src/local.dart';
import 'package:shared/shared.dart';
export 'src/missing.dart';
part 'src/app_part.dart';
"#,
                ),
                DartFileInput::new("packages/app/lib/src/local.dart", "class Local {}"),
                DartFileInput::new(
                    "packages/app/lib/src/app_part.dart",
                    "part of '../app.dart';",
                ),
                DartFileInput::new("packages/shared/lib/shared.dart", "class Shared {}"),
            ],
            vec![
                dartscope_core::PubspecInput::new("packages/app/pubspec.yaml", "name: app\n"),
                dartscope_core::PubspecInput::new("packages/shared/pubspec.yaml", "name: shared\n"),
            ],
        ));

        let graph = build_uri_graph(&project);

        assert_eq!(graph.references.len(), 5);
        assert_eq!(graph.references[0].resolution, DartUriResolution::External);
        assert_eq!(
            graph.references[1].target_path.as_deref(),
            Some("packages/app/lib/src/local.dart")
        );
        assert_eq!(graph.references[1].resolution, DartUriResolution::Resolved);
        assert_eq!(
            graph.references[2].target_path.as_deref(),
            Some("packages/shared/lib/shared.dart")
        );
        assert_eq!(graph.references[2].resolution, DartUriResolution::Resolved);
        assert_eq!(
            graph.references[3].resolution,
            DartUriResolution::MissingTarget
        );
        assert_eq!(graph.references[4].kind, DartUriReferenceKind::Part);
        assert_eq!(graph.references[4].resolution, DartUriResolution::Resolved);
    }

    #[test]
    fn uri_graph_json_schema_fixture_is_stable() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/client.dart", "import 'docs.dart' show viewerQuery;\n"),
                DartFileInput::new("lib/docs.dart", "const viewerQuery = 'query Viewer';\n"),
            ],
            vec![],
        ));

        let graph = build_uri_graph(&project);

        assert_eq!(
            serde_json::to_value(&graph).unwrap(),
            serde_json::json!({
                "references": [
                    {
                        "source_path": "lib/client.dart",
                        "source_span": {
                            "byte_start": 0,
                            "byte_end": 36,
                            "start_line": 1,
                            "start_column": 1,
                            "end_line": 1,
                            "end_column": 37
                        },
                        "uri": "docs.dart",
                        "condition": null,
                        "kind": "import",
                        "resolution": "resolved",
                        "target_path": "lib/docs.dart",
                        "target_uri": null,
                        "candidate_paths": []
                    }
                ]
            })
        );
    }

    #[test]
    fn part_links_json_schema_fixture_is_stable() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/models.dart", "part 'src/model.dart';\n"),
                DartFileInput::new(
                    "lib/src/model.dart",
                    "part of '../models.dart';\nclass Model {}\n",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_part_links(&project);

        assert_eq!(
            serde_json::to_value(&analysis).unwrap(),
            serde_json::json!({
                "links": [
                    {
                        "owner_path": "lib/models.dart",
                        "part_uri": "src/model.dart",
                        "part_path": "lib/src/model.dart",
                        "declared_owner": "../models.dart",
                        "status": "matched",
                        "part_span": {
                            "byte_start": 0,
                            "byte_end": 22,
                            "start_line": 1,
                            "start_column": 1,
                            "end_line": 1,
                            "end_column": 23
                        },
                        "part_of_span": {
                            "byte_start": 0,
                            "byte_end": 25,
                            "start_line": 1,
                            "start_column": 1,
                            "end_line": 1,
                            "end_column": 26
                        }
                    }
                ]
            })
        );
    }

    #[test]
    fn graphql_contract_json_schema_fixture_is_stable() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/client.dart",
                "const viewerQuery = r'''\nquery Viewer($id: ID!) { viewer(id: $id) { id } }\n''';\n\nvoid load() {\n  client.query(QueryOptions(document: gql(viewerQuery), variables: {'id': id, 'extra': true}));\n}\n",
            )],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert_eq!(
            serde_json::to_value(&analysis).unwrap(),
            serde_json::json!({
                "bindings": [
                    {
                        "constant_name": "viewerQuery",
                        "resolution_basis": "same_file",
                        "operation_name": "Viewer",
                        "operation_type": "query",
                        "client_call": "query",
                        "call_compatibility": "match",
                        "declared_variable_names": ["id"],
                        "supplied_variable_names": ["extra", "id"],
                        "missing_variable_names": [],
                        "unexpected_variable_names": ["extra"],
                        "variable_compatibility": "mismatch",
                        "operation_path": "lib/client.dart",
                        "operation_span": {
                            "byte_start": 0,
                            "byte_end": 24,
                            "start_line": 1,
                            "start_column": 1,
                            "end_line": 1,
                            "end_column": 25
                        },
                        "use_path": "lib/client.dart",
                        "use_span": {
                            "byte_start": 95,
                            "byte_end": 190,
                            "start_line": 6,
                            "start_column": 1,
                            "end_line": 6,
                            "end_column": 96
                        },
                        "enclosing_symbol": {
                            "name": "load",
                            "kind": "callable"
                        }
                    }
                ],
                "unresolved_uses": []
            })
        );
    }

    #[test]
    fn reports_unknown_and_ambiguous_packages() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/use.dart",
                "import 'package:missing/api.dart';\nimport 'package:duplicate/api.dart';",
            )],
            vec![
                dartscope_core::PubspecInput::new("one/pubspec.yaml", "name: duplicate\n"),
                dartscope_core::PubspecInput::new("two/pubspec.yaml", "name: duplicate\n"),
            ],
        ));

        let graph = build_uri_graph(&project);

        assert_eq!(
            graph.references[0].resolution,
            DartUriResolution::UnindexedPackage
        );
        assert_eq!(
            graph.references[1].resolution,
            DartUriResolution::AmbiguousPackage
        );
        assert_eq!(
            graph.references[1].candidate_paths,
            ["one/lib/api.dart", "two/lib/api.dart"]
        );
    }

    #[test]
    fn resolves_every_conditional_uri_without_selecting_an_environment() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/platform.dart",
                    "import 'src/stub.dart' if (dart.library.io) 'src/io.dart' if (dart.library.js_interop) 'src/web.dart';\n",
                ),
                DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/io.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/web.dart", "class PlatformApi {}\n"),
            ],
            vec![],
        ));

        let graph = build_uri_graph(&project);

        assert_eq!(graph.references.len(), 3);
        assert!(graph
            .references
            .iter()
            .all(|reference| reference.resolution == DartUriResolution::Resolved));
        assert!(graph.references.iter().any(|reference| {
            reference.uri == "src/stub.dart" && reference.condition.is_none()
        }));
        assert!(graph.references.iter().any(|reference| {
            reference.uri == "src/io.dart"
                && reference.condition.as_deref() == Some("dart.library.io")
        }));
        assert!(graph.references.iter().any(|reference| {
            reference.uri == "src/web.dart"
                && reference.condition.as_deref() == Some("dart.library.js_interop")
        }));
    }

    #[test]
    fn selects_the_first_matching_conditional_uri_when_environment_is_explicit() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/platform.dart",
                    "import 'src/stub.dart' if (flavor == 'prod') 'src/prod.dart' if (flavor == 'dev') 'src/dev.dart';\n",
                ),
                DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/prod.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/dev.dart", "class PlatformApi {}\n"),
            ],
            vec![],
        ));
        let options = DartIndexOptions::default().with_compilation_environment(
            DartCompilationEnvironment::from_pairs([("flavor", "prod")]),
        );

        let graph = build_uri_graph_with_options(&project, &options);

        assert_eq!(graph.references.len(), 1);
        assert_eq!(graph.references[0].uri, "src/prod.dart");
        assert_eq!(
            graph.references[0].condition.as_deref(),
            Some("flavor == 'prod'")
        );
        assert_eq!(graph.references[0].resolution, DartUriResolution::Resolved);
    }

    #[test]
    fn falls_back_to_default_conditional_uri_when_environment_does_not_match() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/platform.dart",
                    "import 'src/stub.dart' if (dart.library.io) 'src/io.dart';\n",
                ),
                DartFileInput::new("lib/src/stub.dart", "class PlatformApi {}\n"),
                DartFileInput::new("lib/src/io.dart", "class PlatformApi {}\n"),
            ],
            vec![],
        ));
        let options = DartIndexOptions::default().with_compilation_environment(
            DartCompilationEnvironment::from_pairs([("dart.library.io", "false")]),
        );

        let graph = build_uri_graph_with_options(&project, &options);

        assert_eq!(graph.references.len(), 1);
        assert_eq!(graph.references[0].uri, "src/stub.dart");
        assert_eq!(graph.references[0].condition, None);
        assert_eq!(graph.references[0].resolution, DartUriResolution::Resolved);
    }

    #[test]
    fn resolves_package_uris_through_the_nearest_package_config() {
        let project = analyze_project(
            DartProjectInput::new(
                ".",
                vec![
                    DartFileInput::new(
                        "apps/demo/lib/client.dart",
                        "import 'package:shared/api.dart';\nimport 'package:graphql/client.dart';\n",
                    ),
                    DartFileInput::new("packages/shared/lib/api.dart", "class Api {}\n"),
                ],
                vec![],
            )
            .with_package_configs(vec![PackageConfigInput::new(
                "apps/demo/.dart_tool/package_config.json",
                r#"{
  "configVersion": 2,
  "packages": [
    {"name":"shared","rootUri":"../../../packages/shared/","packageUri":"lib/"},
    {"name":"graphql","rootUri":"file:///cache/graphql-5.2.0/","packageUri":"lib/"}
  ]
}"#,
            )]),
        );

        let graph = build_uri_graph(&project);
        let shared = graph
            .references
            .iter()
            .find(|reference| reference.uri == "package:shared/api.dart")
            .unwrap();
        assert_eq!(shared.resolution, DartUriResolution::Resolved);
        assert_eq!(
            shared.target_path.as_deref(),
            Some("packages/shared/lib/api.dart")
        );
        assert_eq!(
            shared.target_uri.as_deref(),
            Some("file:///__dartscope_project__/packages/shared/lib/api.dart")
        );

        let graphql = graph
            .references
            .iter()
            .find(|reference| reference.uri == "package:graphql/client.dart")
            .unwrap();
        assert_eq!(graphql.resolution, DartUriResolution::ResolvedExternal);
        assert_eq!(graphql.target_path, None);
        assert_eq!(
            graphql.target_uri.as_deref(),
            Some("file:///cache/graphql-5.2.0/lib/client.dart")
        );
    }

    #[test]
    fn a_nested_package_config_overrides_an_ancestor_config() {
        let project = analyze_project(
            DartProjectInput::new(
                ".",
                vec![DartFileInput::new(
                    "apps/demo/lib/client.dart",
                    "import 'package:shared/api.dart';\n",
                )],
                vec![],
            )
            .with_package_configs(vec![
                PackageConfigInput::new(
                    ".dart_tool/package_config.json",
                    r#"{"configVersion":2,"packages":[{"name":"shared","rootUri":"../packages/shared/","packageUri":"lib/"}]}"#,
                ),
                PackageConfigInput::new(
                    "apps/demo/.dart_tool/package_config.json",
                    r#"{"configVersion":2,"packages":[{"name":"shared","rootUri":"file:///nested/shared/","packageUri":"lib/"}]}"#,
                ),
            ]),
        );

        let graph = build_uri_graph(&project);

        assert_eq!(graph.references.len(), 1);
        assert_eq!(
            graph.references[0].resolution,
            DartUriResolution::ResolvedExternal
        );
        assert_eq!(
            graph.references[0].target_uri.as_deref(),
            Some("file:///nested/shared/lib/api.dart")
        );
    }

    #[test]
    fn does_not_fall_back_when_the_nearest_package_config_is_invalid() {
        let project = analyze_project(
            DartProjectInput::new(
                ".",
                vec![DartFileInput::new(
                    "apps/demo/lib/client.dart",
                    "import 'package:shared/api.dart';\n",
                )],
                vec![],
            )
            .with_package_configs(vec![PackageConfigInput::new(
                "apps/demo/.dart_tool/package_config.json",
                r#"{"configVersion":3,"packages":[]}"#,
            )]),
        );

        let graph = build_uri_graph(&project);

        assert_eq!(
            graph.references[0].resolution,
            DartUriResolution::InvalidConfiguration
        );
    }

    #[test]
    fn requires_an_environment_before_resolving_a_conditional_namespace() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/stub.dart",
                    "const viewerQuery = r'''query StubViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/io.dart",
                    "const viewerQuery = r'''query IoViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'stub.dart' if (dart.library.io) 'io.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 1);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::ConditionalEnvironmentRequired
        );
    }

    #[test]
    fn resolves_a_conditional_namespace_when_environment_is_explicit() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/stub.dart",
                    "const viewerQuery = r'''query StubViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/io.dart",
                    "const viewerQuery = r'''query IoViewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'stub.dart' if (dart.library.io) 'io.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));
        let options = DartIndexOptions::default().with_compilation_environment(
            DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
        );

        let analysis = analyze_graphql_contracts_with_options(&project, &options);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(analysis.bindings[0].operation_path, "lib/io.dart");
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::DirectImport
        );
    }

    #[test]
    fn validates_uri_and_named_part_ownership() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/models.dart",
                    "library app.models;\npart 'src/model.dart';\npart 'src/named.dart';\n",
                ),
                DartFileInput::new(
                    "lib/src/model.dart",
                    "part of '../models.dart';\nclass Model {}\n",
                ),
                DartFileInput::new(
                    "lib/src/named.dart",
                    "part of app.models;\nclass Named {}\n",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_part_links(&project);

        assert_eq!(analysis.links.len(), 2);
        assert!(analysis
            .links
            .iter()
            .all(|link| link.status == DartPartLinkStatus::Matched));
        assert!(analysis
            .links
            .iter()
            .all(|link| link.part_of_span.is_some()));
    }

    #[test]
    fn reports_invalid_part_links_with_evidence() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/library.dart",
                    "part 'missing.dart';\npart 'plain.dart';\npart 'wrong.dart';\n",
                ),
                DartFileInput::new("lib/plain.dart", "class Plain {}\n"),
                DartFileInput::new("lib/wrong.dart", "part of 'other.dart';\nclass Wrong {}\n"),
            ],
            vec![],
        ));

        let analysis = analyze_part_links(&project);
        let statuses: Vec<_> = analysis.links.iter().map(|link| link.status).collect();

        assert_eq!(
            statuses,
            [
                DartPartLinkStatus::MissingTarget,
                DartPartLinkStatus::MissingPartOf,
                DartPartLinkStatus::DifferentLibrary,
            ]
        );
        assert!(analysis.links[0].part_span.start_line > 0);
        assert!(analysis.links[2].part_of_span.is_some());
    }

    #[test]
    fn does_not_treat_an_unindexed_package_part_as_a_missing_file() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/library.dart",
                "part 'package:generated/models.dart';\n",
            )],
            vec![],
        ));

        let analysis = analyze_part_links(&project);

        assert_eq!(analysis.links.len(), 1);
        assert_eq!(
            analysis.links[0].status,
            DartPartLinkStatus::UnresolvedTarget
        );
        assert_eq!(analysis.links[0].part_path, None);
    }

    #[test]
    fn binds_operations_and_compares_call_and_variable_contracts() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/api.dart",
                r#"
const updateUserMutation = r'''
  mutation UpdateUser($id: ID!, $input: UserInput!) {
    updateUser(id: $id, input: $input) { id }
  }
''';

Future<void> updateUser() async {
  await client.query(QueryOptions(
    document: gql(updateUserMutation),
    variables: <String, dynamic>{'id': id, 'extra': true},
  ));
}
"#,
            )],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        let binding = &analysis.bindings[0];
        assert_eq!(
            binding.resolution_basis,
            DartGraphqlBindingResolution::SameFile
        );
        assert_eq!(
            binding.call_compatibility,
            DartGraphqlCallCompatibility::Mismatch
        );
        assert_eq!(
            binding.variable_compatibility,
            DartGraphqlVariableCompatibility::Mismatch
        );
        assert_eq!(binding.missing_variable_names, ["input"]);
        assert_eq!(binding.unexpected_variable_names, ["extra"]);
        assert_eq!(binding.operation_path, "lib/api.dart");
        assert_eq!(binding.use_path, "lib/api.dart");
    }

    #[test]
    fn reports_missing_and_non_visible_declarations_without_guessing() {
        let operation = r#"
const sharedQuery = r'''query Shared { viewer { id } }''';
"#;
        let usage = r#"
void load() {
  client.query(QueryOptions(document: gql(sharedQuery)));
  client.query(QueryOptions(document: gql(missingQuery)));
}
"#;
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/a.dart", operation),
                DartFileInput::new("lib/b.dart", operation),
                DartFileInput::new("lib/use.dart", usage),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 2);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration
        );
        assert_eq!(
            analysis.unresolved_uses[0].candidate_paths,
            ["lib/a.dart", "lib/b.dart"]
        );
        assert_eq!(
            analysis.unresolved_uses[1].reason,
            DartGraphqlUnresolvedReason::MissingDeclaration
        );
    }

    #[test]
    fn same_file_declaration_wins_over_duplicate_names_in_other_files() {
        let local = r#"
const sharedQuery = r'''query LocalShared { localViewer { id } }''';

void load() {
  client.query(QueryOptions(document: gql(sharedQuery)));
}
"#;
        let duplicate = "const sharedQuery = r'''query OtherShared { otherViewer { id } }''';\n";
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/local.dart", local),
                DartFileInput::new("lib/other.dart", duplicate),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(
            analysis.bindings[0].operation_name.as_deref(),
            Some("LocalShared")
        );
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::SameFile
        );
    }

    #[test]
    fn does_not_resolve_a_cross_file_name_without_an_import() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/document.dart",
                    "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "void load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 1);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration
        );
    }

    #[test]
    fn resolves_an_unqualified_operation_through_a_direct_import() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/documents.dart",
                    "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/other.dart",
                    "const viewerQuery = r'''query OtherViewer { otherViewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'documents.dart' show viewerQuery;\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(
            analysis.bindings[0].operation_name.as_deref(),
            Some("Viewer")
        );
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::DirectImport
        );
    }

    #[test]
    fn respects_prefix_show_and_hide_when_resolving_direct_imports() {
        let operation = "const viewerQuery = r'''query Viewer { viewer { id } }''';\n";
        for import in [
            "import 'documents.dart' as docs;",
            "import 'documents.dart' hide viewerQuery;",
            "import 'documents.dart' show otherQuery;",
        ] {
            let project = analyze_project(DartProjectInput::new(
                ".",
                vec![
                    DartFileInput::new("lib/documents.dart", operation),
                    DartFileInput::new("lib/duplicate.dart", operation),
                    DartFileInput::new(
                        "lib/client.dart",
                        format!(
                            "{import}\nvoid load() {{ client.query(QueryOptions(document: gql(viewerQuery))); }}"
                        ),
                    ),
                ],
                vec![],
            ));

            let analysis = analyze_graphql_contracts(&project);

            assert!(
                analysis.bindings.is_empty(),
                "unexpected binding for {import}"
            );
            assert_eq!(analysis.unresolved_uses.len(), 1);
            assert_eq!(
                analysis.unresolved_uses[0].reason,
                DartGraphqlUnresolvedReason::NotVisibleDeclaration
            );
        }
    }

    #[test]
    fn resolves_an_operation_through_a_re_export_namespace() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/src/documents.dart",
                    "const viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/api.dart",
                    "export 'src/documents.dart' show viewerQuery;\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'api.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::ReExport
        );
        assert_eq!(
            analysis.bindings[0].operation_path,
            "lib/src/documents.dart"
        );
    }

    #[test]
    fn reports_ambiguous_imported_operations_and_ignores_private_exports() {
        let public_operation = "const viewerQuery = r'''query Viewer { viewer { id } }''';\n";
        let private_operation =
            "const _privateQuery = r'''query PrivateViewer { viewer { id } }''';\n";
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/a.dart", public_operation),
                DartFileInput::new("lib/b.dart", public_operation),
                DartFileInput::new("lib/private.dart", private_operation),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'a.dart';\nimport 'b.dart';\nimport 'private.dart';\nvoid load() {\n  client.query(QueryOptions(document: gql(viewerQuery)));\n  client.query(QueryOptions(document: gql(_privateQuery)));\n}",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 2);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::AmbiguousDeclaration
        );
        assert_eq!(
            analysis.unresolved_uses[1].reason,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration
        );
    }

    #[test]
    fn resolves_operations_between_validated_sibling_parts() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/api.dart",
                    "part 'src/documents.dart';\npart 'src/client.dart';\n",
                ),
                DartFileInput::new(
                    "lib/src/documents.dart",
                    "part of '../api.dart';\nconst viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/src/client.dart",
                    "part of '../api.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::SameLibrary
        );
        assert_eq!(
            analysis.bindings[0].operation_path,
            "lib/src/documents.dart"
        );
        assert_eq!(analysis.bindings[0].use_path, "lib/src/client.dart");
    }

    #[test]
    fn imports_public_operations_declared_in_a_validated_part() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/api.dart", "part 'src/documents.dart';\n"),
                DartFileInput::new(
                    "lib/src/documents.dart",
                    "part of '../api.dart';\nconst viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'api.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.unresolved_uses.is_empty());
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(
            analysis.bindings[0].resolution_basis,
            DartGraphqlBindingResolution::DirectImport
        );
        assert_eq!(
            analysis.bindings[0].operation_path,
            "lib/src/documents.dart"
        );
    }

    #[test]
    fn excludes_a_part_that_declares_a_different_owner() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/api.dart", "part 'src/documents.dart';\n"),
                DartFileInput::new(
                    "lib/src/documents.dart",
                    "part of '../other.dart';\nconst viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
                DartFileInput::new(
                    "lib/client.dart",
                    "import 'api.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 1);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration
        );
    }

    #[test]
    fn does_not_assign_a_named_part_claimed_by_multiple_libraries() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new(
                    "lib/a.dart",
                    "library shared;\npart 'shared.dart';\nvoid load() { client.query(QueryOptions(document: gql(viewerQuery))); }",
                ),
                DartFileInput::new(
                    "lib/b.dart",
                    "library shared;\npart 'shared.dart';\n",
                ),
                DartFileInput::new(
                    "lib/shared.dart",
                    "part of shared;\nconst viewerQuery = r'''query Viewer { viewer { id } }''';\n",
                ),
            ],
            vec![],
        ));

        let analysis = analyze_graphql_contracts(&project);

        assert!(analysis.bindings.is_empty());
        assert_eq!(analysis.unresolved_uses.len(), 1);
        assert_eq!(
            analysis.unresolved_uses[0].reason,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration
        );
    }
}
