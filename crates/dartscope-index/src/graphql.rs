use std::collections::HashMap;

use dartscope_core::{
    DartEnclosingSymbol, DartFileAnalysis, DartGraphqlBindingResolution,
    DartGraphqlCallCompatibility, DartGraphqlClientCall, DartGraphqlContractAnalysis,
    DartGraphqlOperation, DartGraphqlOperationBinding, DartGraphqlOperationType,
    DartGraphqlOperationUse, DartGraphqlUnresolvedOperationUse, DartGraphqlUnresolvedReason,
    DartGraphqlVariableCompatibility, DartProjectAnalysis, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus, SourceSpan,
};

use crate::namespace::{NamespaceCandidate, NamespaceResolution, NamespaceResolver};
use crate::uri_graph::DartIndexOptions;

struct OperationLocation<'a> {
    path: &'a str,
    operation: &'a DartGraphqlOperation,
}

pub fn analyze_graphql_contracts(project: &DartProjectAnalysis) -> DartGraphqlContractAnalysis {
    analyze_graphql_contracts_with_options(project, &DartIndexOptions::default())
}

pub fn analyze_graphql_contracts_with_options(
    project: &DartProjectAnalysis,
    options: &DartIndexOptions,
) -> DartGraphqlContractAnalysis {
    let resolver = NamespaceResolver::new(project, options);
    let operations_by_constant = collect_operations(project);
    let mut analysis = DartGraphqlContractAnalysis::default();

    for file in &project.files {
        for operation_use in &file.graphql_operation_uses {
            let candidates = operations_by_constant
                .get(operation_use.constant_name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            record_use(&resolver, &mut analysis, file, operation_use, candidates);
        }
    }

    sort_contract_analysis(&mut analysis);
    analysis
}

fn collect_operations<'source>(
    project: &'source DartProjectAnalysis,
) -> HashMap<&'source str, Vec<OperationLocation<'source>>> {
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

fn record_use<'source>(
    resolver: &NamespaceResolver<'source, '_>,
    analysis: &mut DartGraphqlContractAnalysis,
    file: &'source DartFileAnalysis,
    operation_use: &DartGraphqlOperationUse,
    candidates: &[OperationLocation<'source>],
) {
    let namespace_candidates: Vec<_> = candidates
        .iter()
        .map(|candidate| NamespaceCandidate {
            path: candidate.path,
            byte_start: candidate.operation.span.byte_start,
        })
        .collect();
    let resolution = resolver.resolve(
        &file.path,
        operation_use.constant_name.as_str(),
        None,
        &namespace_candidates,
    );
    record_candidate_resolution(analysis, file, operation_use, candidates, &resolution);
}

fn record_candidate_resolution(
    analysis: &mut DartGraphqlContractAnalysis,
    file: &DartFileAnalysis,
    operation_use: &DartGraphqlOperationUse,
    candidates: &[OperationLocation<'_>],
    resolution: &NamespaceResolution,
) {
    match resolution.status {
        DartSymbolResolutionStatus::Resolved => {
            let Some((location, basis)) = resolved_candidate(candidates, resolution) else {
                push_unresolved_use(
                    analysis,
                    operation_use,
                    &file.path,
                    DartGraphqlUnresolvedReason::MissingDeclaration,
                    &[],
                );
                return;
            };
            analysis.bindings.push(build_binding(
                location,
                basis,
                &file.path,
                operation_use.client_call,
                &operation_use.variable_names,
                &operation_use.span,
                operation_use.enclosing_symbol.clone(),
            ));
        }
        DartSymbolResolutionStatus::Ambiguous => push_unresolved_use(
            analysis,
            operation_use,
            &file.path,
            DartGraphqlUnresolvedReason::AmbiguousDeclaration,
            &resolution_locations(candidates, resolution),
        ),
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => push_unresolved_use(
            analysis,
            operation_use,
            &file.path,
            DartGraphqlUnresolvedReason::ConditionalEnvironmentRequired,
            &resolution_locations(candidates, resolution),
        ),
        DartSymbolResolutionStatus::Missing | DartSymbolResolutionStatus::SourceFileMissing => {
            push_unresolved_use(
                analysis,
                operation_use,
                &file.path,
                DartGraphqlUnresolvedReason::MissingDeclaration,
                &[],
            );
        }
        DartSymbolResolutionStatus::NotVisible => push_unresolved_use(
            analysis,
            operation_use,
            &file.path,
            DartGraphqlUnresolvedReason::NotVisibleDeclaration,
            &resolution_locations(candidates, resolution),
        ),
    }
}

fn resolved_candidate<'candidate, 'source>(
    candidates: &'candidate [OperationLocation<'source>],
    resolution: &NamespaceResolution,
) -> Option<(
    &'candidate OperationLocation<'source>,
    DartGraphqlBindingResolution,
)> {
    let [candidate] = resolution.candidates.as_slice() else {
        return None;
    };
    let location = candidates.get(candidate.index)?;
    let basis = graphql_binding_basis(candidate.basis)?;
    Some((location, basis))
}

fn resolution_locations<'candidate, 'source>(
    candidates: &'candidate [OperationLocation<'source>],
    resolution: &NamespaceResolution,
) -> Vec<&'candidate OperationLocation<'source>> {
    resolution
        .candidates
        .iter()
        .filter_map(|candidate| candidates.get(candidate.index))
        .collect()
}

fn graphql_binding_basis(basis: DartSymbolResolutionBasis) -> Option<DartGraphqlBindingResolution> {
    match basis {
        DartSymbolResolutionBasis::SameFile => Some(DartGraphqlBindingResolution::SameFile),
        DartSymbolResolutionBasis::SameLibrary => Some(DartGraphqlBindingResolution::SameLibrary),
        DartSymbolResolutionBasis::DirectImport => Some(DartGraphqlBindingResolution::DirectImport),
        DartSymbolResolutionBasis::ReExport => Some(DartGraphqlBindingResolution::ReExport),
        DartSymbolResolutionBasis::NotVisible => None,
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
    operation_use: &DartGraphqlOperationUse,
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
