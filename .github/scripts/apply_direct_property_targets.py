from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement anchor, found {count}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/dartscope-core/src/lib.rs",
    """    MemberInvocationInstance,\n    MemberInvocationStatic,\n    TypeAnnotation,\n""",
    """    MemberInvocationInstance,\n    MemberInvocationStatic,\n    MemberPropertyDeclarationInstance,\n    MemberPropertyDeclarationStatic,\n    MemberPropertyReadInstance,\n    MemberPropertyReadStatic,\n    MemberPropertyWriteInstance,\n    MemberPropertyWriteStatic,\n    TypeAnnotation,\n""",
)

replace_once(
    "crates/dartscope-parse/src/lib.rs",
    "mod member_references;\nmod namespace;\n",
    "mod member_references;\nmod namespace;\nmod property_references;\n",
)

analysis = Path("crates/dartscope-parse/src/analysis.rs")
text = analysis.read_text()
if "collect_property_references" not in text:
    text = text.replace(
        "use crate::member_references::collect_method_references;\n",
        "use crate::member_references::collect_method_references;\nuse crate::property_references::collect_property_references;\n",
        1,
    )
    first = """    references.extend(collect_method_references(\n        &source,\n        &lexical.code,\n        &file,\n        &bindings,\n    ));\n"""
    first_new = first + """    references.extend(collect_property_references(\n        &source,\n        &lexical.code,\n        &file,\n        &bindings,\n    ));\n"""
    if text.count(first) != 1:
        raise SystemExit("analysis.rs: missing file property collector anchor")
    text = text.replace(first, first_new, 1)
    second = """        file_references.extend(collect_method_references(\n            source,\n            &lexical.code,\n            file,\n            &file_bindings,\n        ));\n"""
    second_new = second + """        file_references.extend(collect_property_references(\n            source,\n            &lexical.code,\n            file,\n            &file_bindings,\n        ));\n"""
    if text.count(second) != 1:
        raise SystemExit("analysis.rs: missing project property collector anchor")
    text = text.replace(second, second_new, 1)
    analysis.write_text(text)

replace_once(
    "crates/dartscope-index/src/references.rs",
    """                    | DartIdentifierReferenceKind::MemberInvocationInstance\n                    | DartIdentifierReferenceKind::MemberInvocationStatic\n""",
    """                    | DartIdentifierReferenceKind::MemberInvocationInstance\n                    | DartIdentifierReferenceKind::MemberInvocationStatic\n                    | DartIdentifierReferenceKind::MemberPropertyDeclarationInstance\n                    | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic\n                    | DartIdentifierReferenceKind::MemberPropertyReadInstance\n                    | DartIdentifierReferenceKind::MemberPropertyReadStatic\n                    | DartIdentifierReferenceKind::MemberPropertyWriteInstance\n                    | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n""",
)

navigation = Path("crates/dartscope-index/src/navigation.rs")
text = navigation.read_text()
if "struct IndexedProperty" not in text:
    text = text.replace(
        """struct IndexedMethod {\n    owner_symbol_id: String,\n    is_static: bool,\n    candidate: DartSymbolCandidate,\n}\n\n#[derive(Debug, Clone, Default)]\nstruct MemberIndex {\n    methods: Vec<IndexedMethod>,\n}\n""",
        """struct IndexedMethod {\n    owner_symbol_id: String,\n    is_static: bool,\n    candidate: DartSymbolCandidate,\n}\n\n#[derive(Debug, Clone, Eq, PartialEq)]\nstruct IndexedProperty {\n    owner_symbol_id: String,\n    is_static: bool,\n    candidate: DartSymbolCandidate,\n}\n\n#[derive(Debug, Clone, Default)]\nstruct MemberIndex {\n    methods: Vec<IndexedMethod>,\n    properties: Vec<IndexedProperty>,\n}\n""",
        1,
    )
    old = """        methods.dedup();\n        Self { methods }\n"""
    new = """        methods.dedup();\n        let mut properties = analysis\n            .references\n            .iter()\n            .filter_map(|reference| {\n                let is_static = match reference.kind {\n                    DartIdentifierReferenceKind::MemberPropertyDeclarationInstance => false,\n                    DartIdentifierReferenceKind::MemberPropertyDeclarationStatic => true,\n                    _ => return None,\n                };\n                let owner_symbol_id = reference.prefix.clone()?;\n                let file = analysis\n                    .project\n                    .files\n                    .iter()\n                    .find(|file| file.path == reference.source_path)?;\n                let declaration = file.declarations.iter().find(|declaration| {\n                    matches!(\n                        declaration.kind,\n                        DartDeclarationKind::Field\n                            | DartDeclarationKind::Getter\n                            | DartDeclarationKind::Setter\n                    ) && declaration.name == reference.name\n                        && declaration.parent_symbol_id.as_deref()\n                            == Some(owner_symbol_id.as_str())\n                        && declaration_span_contains(declaration, &reference.span)\n                })?;\n                Some(IndexedProperty {\n                    owner_symbol_id,\n                    is_static,\n                    candidate: declaration_candidate(\n                        file.path.as_str(),\n                        declaration,\n                        DartSymbolResolutionBasis::SameFile,\n                    ),\n                })\n            })\n            .collect::<Vec<_>>();\n        properties.sort_by(|left, right| {\n            (\n                &left.owner_symbol_id,\n                left.is_static,\n                &left.candidate.declaration_path,\n                left.candidate.declaration_span.byte_start,\n                &left.candidate.name,\n                left.candidate.kind,\n            )\n                .cmp(&(\n                    &right.owner_symbol_id,\n                    right.is_static,\n                    &right.candidate.declaration_path,\n                    right.candidate.declaration_span.byte_start,\n                    &right.candidate.name,\n                    right.candidate.kind,\n                ))\n        });\n        properties.dedup();\n        Self {\n            methods,\n            properties,\n        }\n"""
    if text.count(old) != 1:
        raise SystemExit("navigation.rs: missing MemberIndex construction anchor")
    text = text.replace(old, new, 1)

if "resolve_property_declaration_reference" not in text:
    old = """    if is_member_declaration_kind(reference.kind) {\n        return resolve_method_declaration_reference(member_index, reference);\n    }\n"""
    new = """    if is_method_declaration_kind(reference.kind) {\n        return resolve_method_declaration_reference(member_index, reference);\n    }\n    if is_property_declaration_kind(reference.kind) {\n        return resolve_property_declaration_reference(member_index, reference);\n    }\n"""
    if text.count(old) != 1:
        raise SystemExit("navigation.rs: missing declaration dispatch anchor")
    text = text.replace(old, new, 1)
    old = """        );\n    }\n    if matches!(\n        reference.kind,\n        DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite\n    ) {\n"""
    new = """        );\n    }\n    if is_property_access_kind(reference.kind) {\n        return resolve_property_access_reference(\n            analysis,\n            namespace,\n            uri_graph,\n            member_index,\n            reference,\n        );\n    }\n    if matches!(\n        reference.kind,\n        DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite\n    ) {\n"""
    if text.count(old) != 1:
        raise SystemExit("navigation.rs: missing property access dispatch anchor")
    text = text.replace(old, new, 1)

    helper_old = """fn is_member_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberDeclarationInstance\n            | DartIdentifierReferenceKind::MemberDeclarationStatic\n    )\n}\n\n"""
    helper_new = """fn is_method_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberDeclarationInstance\n            | DartIdentifierReferenceKind::MemberDeclarationStatic\n    )\n}\n\nfn is_property_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberPropertyDeclarationInstance\n            | DartIdentifierReferenceKind::MemberPropertyDeclarationStatic\n    )\n}\n\nfn is_member_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {\n    is_method_declaration_kind(kind) || is_property_declaration_kind(kind)\n}\n\nfn is_property_access_kind(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberPropertyReadInstance\n            | DartIdentifierReferenceKind::MemberPropertyReadStatic\n            | DartIdentifierReferenceKind::MemberPropertyWriteInstance\n            | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n    )\n}\n\nfn property_access_is_static(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberPropertyReadStatic\n            | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n    )\n}\n\nfn property_access_is_write(kind: DartIdentifierReferenceKind) -> bool {\n    matches!(\n        kind,\n        DartIdentifierReferenceKind::MemberPropertyWriteInstance\n            | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n    )\n}\n\n"""
    if text.count(helper_old) != 1:
        raise SystemExit("navigation.rs: missing member helper anchor")
    text = text.replace(helper_old, helper_new, 1)

    property_block = r'''// Direct property resolution uses only parser-produced owner and access-mode facts.
fn resolve_property_declaration_reference(
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let is_static =
        reference.kind == DartIdentifierReferenceKind::MemberPropertyDeclarationStatic;
    let owner_symbol_id = reference.prefix.as_deref();
    let mut targets = member_index
        .properties
        .iter()
        .filter(|property| {
            Some(property.owner_symbol_id.as_str()) == owner_symbol_id
                && property.is_static == is_static
                && property.candidate.name == reference.name
                && property.candidate.declaration_path == reference.source_path
                && property.candidate.declaration_span.byte_start <= reference.span.byte_start
                && reference.span.byte_end <= property.candidate.declaration_span.byte_end
        })
        .map(|property| DartDefinitionTarget::Namespace(property.candidate.clone()))
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    let status = match targets.len() {
        0 => DartDefinitionResolutionStatus::Missing,
        1 => DartDefinitionResolutionStatus::Resolved,
        _ => DartDefinitionResolutionStatus::Ambiguous,
    };
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris: Vec::new(),
    }
}

fn resolve_property_access_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    if property_access_is_static(reference.kind) {
        resolve_static_property_reference(analysis, namespace, uri_graph, member_index, reference)
    } else {
        resolve_instance_property_reference(analysis, namespace, member_index, reference)
    }
}

fn resolve_instance_property_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let owner_symbol_id = reference.prefix.as_deref().unwrap_or_default();
    let mut owners = member_owner_candidates_by_symbol_id(
        analysis,
        namespace,
        &reference.source_path,
        owner_symbol_id,
    );
    owners.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
            ))
    });
    owners.dedup();
    let is_write = property_access_is_write(reference.kind);
    let refinements = owners
        .iter()
        .map(|owner| {
            refine_property_target(
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                false,
                is_write,
            )
        })
        .collect::<Vec<_>>();
    finish_property_resolution(reference, refinements, Vec::new())
}

fn resolve_static_property_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let Some((import_prefix, owner_name)) = static_member_owner(&reference) else {
        return ResolvedReference {
            reference,
            status: DartDefinitionResolutionStatus::Missing,
            targets: Vec::new(),
            external_uris: Vec::new(),
        };
    };
    let query = DartSymbolQuery {
        source_path: reference.source_path.clone(),
        name: owner_name.clone(),
        prefix: import_prefix.clone(),
    };
    let resolution = resolve_constructible_type_with_resolver(&analysis.project, query, namespace);
    let external_uris = external_member_owner_uris(
        analysis,
        uri_graph,
        &reference,
        owner_name.as_str(),
        import_prefix,
    );
    let base_status = if resolution.status
        == DartSymbolResolutionStatus::ConditionalEnvironmentRequired
        && resolution.candidates.is_empty()
        && !external_uris.is_empty()
    {
        DartDefinitionResolutionStatus::ExternalUnindexed
    } else {
        definition_status(resolution.status, !external_uris.is_empty())
    };
    let is_write = property_access_is_write(reference.kind);
    let refinements = resolution
        .candidates
        .iter()
        .map(|owner| {
            refine_property_target(
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                true,
                is_write,
            )
        })
        .collect::<Vec<_>>();
    if base_status == DartDefinitionResolutionStatus::Resolved {
        finish_property_resolution(reference, refinements, external_uris)
    } else {
        let mut targets = refinements
            .iter()
            .flat_map(|refinement| refinement.targets.iter().cloned())
            .collect::<Vec<_>>();
        targets.sort_by(compare_targets);
        targets.dedup_by(|left, right| same_target(left, right));
        ResolvedReference {
            reference,
            status: base_status,
            targets,
            external_uris,
        }
    }
}

#[derive(Debug)]
struct PropertyRefinement {
    status: DartDefinitionResolutionStatus,
    targets: Vec<DartDefinitionTarget>,
}

fn refine_property_target(
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    property_name: &str,
    is_static: bool,
    is_write: bool,
) -> PropertyRefinement {
    let Some(owner_symbol_id) = owner.symbol_id.as_deref() else {
        return missing_property_target(owner);
    };
    let mut exact = member_index
        .properties
        .iter()
        .filter(|property| {
            property.owner_symbol_id == owner_symbol_id
                && property.is_static == is_static
                && property.candidate.name == property_name
                && property_candidate_matches_access(property.candidate.kind, is_write)
        })
        .map(|property| {
            let mut candidate = property.candidate.clone();
            candidate.basis = owner.basis;
            candidate
        })
        .collect::<Vec<_>>();
    if exact.is_empty() {
        return missing_property_target(owner);
    }
    let visible = !property_name.starts_with('_')
        || exact
            .iter()
            .all(|candidate| namespace.same_library(source_path, &candidate.declaration_path));
    if !visible {
        for candidate in &mut exact {
            candidate.basis = DartSymbolResolutionBasis::NotVisible;
        }
    }
    exact.sort_by(|left, right| {
        (
            &left.declaration_path,
            left.declaration_span.byte_start,
            &left.name,
            left.kind,
            &left.symbol_id,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
                right.kind,
                &right.symbol_id,
            ))
    });
    exact.dedup();
    let status = if !visible {
        DartDefinitionResolutionStatus::NotVisible
    } else if exact.len() == 1 {
        DartDefinitionResolutionStatus::Resolved
    } else {
        DartDefinitionResolutionStatus::Ambiguous
    };
    PropertyRefinement {
        status,
        targets: exact
            .into_iter()
            .map(DartDefinitionTarget::Namespace)
            .collect(),
    }
}

fn property_candidate_matches_access(kind: DartDeclarationKind, is_write: bool) -> bool {
    if is_write {
        matches!(kind, DartDeclarationKind::Field | DartDeclarationKind::Setter)
    } else {
        matches!(kind, DartDeclarationKind::Field | DartDeclarationKind::Getter)
    }
}

fn missing_property_target(owner: &DartSymbolCandidate) -> PropertyRefinement {
    PropertyRefinement {
        status: DartDefinitionResolutionStatus::Missing,
        targets: vec![DartDefinitionTarget::Namespace(owner.clone())],
    }
}

fn finish_property_resolution(
    reference: DartIdentifierReference,
    refinements: Vec<PropertyRefinement>,
    external_uris: Vec<String>,
) -> ResolvedReference {
    let mut targets = refinements
        .iter()
        .flat_map(|refinement| refinement.targets.iter().cloned())
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    let statuses = refinements
        .iter()
        .map(|refinement| refinement.status)
        .collect::<Vec<_>>();
    let status = if statuses.is_empty() {
        DartDefinitionResolutionStatus::Missing
    } else {
        combine_statuses(&statuses, targets.len())
    };
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris,
    }
}

'''
    anchor = "fn resolve_constructor_reference(\n"
    if text.count(anchor) != 1:
        raise SystemExit("navigation.rs: missing constructor anchor")
    text = text.replace(anchor, property_block + anchor, 1)

navigation.write_text(text)
