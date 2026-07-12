use std::collections::{HashMap, HashSet};

use dartscope_core::{
    DartEnclosingSymbol, DartFileAnalysis, DartGraphqlBindingResolution,
    DartGraphqlCallCompatibility, DartGraphqlClientCall, DartGraphqlContractAnalysis,
    DartGraphqlOperation, DartGraphqlOperationBinding, DartGraphqlOperationType,
    DartGraphqlOperationUse, DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason,
    DartGraphqlVariableCompatibility, DartNamespaceCombinatorKind, DartPartLinkStatus,
    DartProjectAnalysis, DartUriGraph, DartUriReferenceKind, DartUriResolution, SourceSpan,
};

use crate::parts::analyze_part_links;
use crate::uri_graph::{build_uri_graph_with_options, DartIndexOptions};

struct OperationLocation<'a> {
    path: &'a str,
    operation: &'a DartGraphqlOperation,
}

struct ImportedOperationCandidate<'candidate, 'source> {
    location: &'candidate OperationLocation<'source>,
    basis: DartGraphqlBindingResolution,
}

struct ImportedOperationResolution<'candidate, 'source> {
    candidates: Vec<ImportedOperationCandidate<'candidate, 'source>>,
    conditional_environment_required: bool,
}

#[derive(Default)]
struct LibraryMembership {
    owner_by_part: HashMap<String, String>,
    members_by_owner: HashMap<String, Vec<String>>,
}

struct ExportResolutionContext<'source, 'candidate, 'context> {
    constant_name: &'context str,
    candidates: &'candidate [OperationLocation<'source>],
    uri_graph: &'context DartUriGraph,
    files_by_path: &'context HashMap<&'source str, &'source DartFileAnalysis>,
    library_membership: &'context LibraryMembership,
    options: &'context DartIndexOptions,
}

struct GraphqlAnalysisContext<'source, 'options> {
    uri_graph: DartUriGraph,
    library_membership: LibraryMembership,
    files_by_path: HashMap<&'source str, &'source DartFileAnalysis>,
    options: &'options DartIndexOptions,
}

struct VisibleOperationCandidates<'candidate, 'source> {
    same_file: Vec<&'candidate OperationLocation<'source>>,
    same_library: Vec<&'candidate OperationLocation<'source>>,
    imported: ImportedOperationResolution<'candidate, 'source>,
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

impl<'source, 'options> GraphqlAnalysisContext<'source, 'options> {
    fn new(project: &'source DartProjectAnalysis, options: &'options DartIndexOptions) -> Self {
        Self {
            uri_graph: build_uri_graph_with_options(project, options),
            library_membership: LibraryMembership::from_project(project),
            files_by_path: project
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect(),
            options,
        }
    }

    fn record_use<'candidate>(
        &self,
        analysis: &mut DartGraphqlContractAnalysis,
        file: &'source DartFileAnalysis,
        operation_use: &DartGraphqlOperationUse,
        candidates: &'candidate [OperationLocation<'source>],
    ) {
        let visible = self.visible_candidates(file, operation_use, candidates);
        record_candidate_resolution(analysis, file, operation_use, candidates, visible);
    }

    fn visible_candidates<'candidate>(
        &self,
        file: &'source DartFileAnalysis,
        operation_use: &DartGraphqlOperationUse,
        candidates: &'candidate [OperationLocation<'source>],
    ) -> VisibleOperationCandidates<'candidate, 'source> {
        let same_file = candidates
            .iter()
            .filter(|location| location.path == file.path)
            .collect();
        let same_library = candidates
            .iter()
            .filter(|location| {
                location.path != file.path
                    && self
                        .library_membership
                        .same_library(location.path, &file.path)
            })
            .collect();
        let namespace_owner = self.library_membership.owner_of(&file.path);
        let namespace_file = self
            .files_by_path
            .get(namespace_owner)
            .copied()
            .unwrap_or(file);
        let imported = imported_operation_candidates(
            namespace_file,
            operation_use.constant_name.as_str(),
            candidates,
            &self.uri_graph,
            &self.files_by_path,
            &self.library_membership,
            self.options,
        );

        VisibleOperationCandidates {
            same_file,
            same_library,
            imported,
        }
    }
}

pub fn analyze_graphql_contracts(project: &DartProjectAnalysis) -> DartGraphqlContractAnalysis {
    analyze_graphql_contracts_with_options(project, &DartIndexOptions::default())
}

pub fn analyze_graphql_contracts_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartGraphqlContractAnalysis {
    let context = GraphqlAnalysisContext::new(project, options);
    let operations_by_constant = collect_operations(project);
    let mut analysis = DartGraphqlContractAnalysis::default();

    for file in &project.files {
        for operation_use in &file.graphql_operation_uses {
            let candidates = operations_by_constant
                .get(operation_use.constant_name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            context.record_use(&mut analysis, file, operation_use, candidates);
        }
    }

    sort_contract_analysis(&mut analysis);
    analysis
}

fn collect_operations(project: &DartProjectAnalysis) -> HashMap<&str, Vec<OperationLocation<'_>>> {
    let mut operations = HashMap::new();
    for file in &project.files {
        for operation in &file.graphql_operations {
            operations
                .entry(operation.constant_name.as_str())
                .or_insert_with(Vec::new)
                .push(OperationLocation {
                    path: &file.path,
                    operation,
                });
        }
    }
    operations
}

fn record_candidate_resolution<'candidate, 'source>(
    analysis: &mut DartGraphqlContractAnalysis,
    file: &DartFileAnalysis,
    operation_use: &DartGraphqlOperationUse,
    candidates: &'candidate [OperationLocation<'source>],
    visible: VisibleOperationCandidates<'candidate, 'source>,
) {
    let VisibleOperationCandidates {
        same_file,
        same_library,
        imported,
    } = visible;

    match (
        same_file.as_slice(),
        same_library.as_slice(),
        imported.candidates.as_slice(),
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
        ([], [], []) if imported.conditional_environment_required => {
            let locations: Vec<_> = candidates.iter().collect();
            push_unresolved_use(
                analysis,
                operation_use,
                &file.path,
                DartGraphqlUnresolvedReason::ConditionalEnvironmentRequired,
                &locations,
            );
        }
        ([], [], []) if candidates.is_empty() => push_unresolved_use(
            analysis,
            operation_use,
            &file.path,
            DartGraphqlUnresolvedReason::MissingDeclaration,
            &[],
        ),
        ([], [], []) => {
            let locations: Vec<_> = candidates.iter().collect();
            push_unresolved_use(
                analysis,
                operation_use,
                &file.path,
                DartGraphqlUnresolvedReason::NotVisibleDeclaration,
                &locations,
            );
        }
        (same_file, same_library, imported) => {
            let locations = ambiguous_locations(same_file, same_library, imported);
            push_unresolved_use(
                analysis,
                operation_use,
                &file.path,
                DartGraphqlUnresolvedReason::AmbiguousDeclaration,
                &locations,
            );
        }
    }
}

fn ambiguous_locations<'candidate, 'source>(
    same_file: &[&'candidate OperationLocation<'source>],
    same_library: &[&'candidate OperationLocation<'source>],
    imported: &[ImportedOperationCandidate<'candidate, 'source>],
) -> Vec<&'candidate OperationLocation<'source>> {
    if !same_file.is_empty() {
        same_file.to_vec()
    } else if !same_library.is_empty() {
        same_library.to_vec()
    } else {
        imported
            .iter()
            .map(|candidate| candidate.location)
            .collect()
    }
}

fn sort_contract_analysis(analysis: &mut DartGraphqlContractAnalysis) {
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

fn imported_operation_candidates<'candidate, 'source>(
    file: &DartFileAnalysis,
    constant_name: &str,
    candidates: &'candidate [OperationLocation<'source>],
    uri_graph: &DartUriGraph,
    files_by_path: &HashMap<&'source str, &'source DartFileAnalysis>,
    library_membership: &LibraryMembership,
    options: &DartIndexOptions,
) -> ImportedOperationResolution<'candidate, 'source> {
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

fn collect_exported_operations<'source, 'candidate>(
    library_path: &str,
    context: &ExportResolutionContext<'source, 'candidate, '_>,
    visited: &mut HashSet<String>,
    conditional_environment_required: &mut bool,
    result: &mut Vec<&'candidate OperationLocation<'source>>,
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
