from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "crates/dartscope-core/src/lib.rs",
    """    InvocationTarget,\n    ConstructorTarget,\n    TypeAnnotation,\n""",
    """    InvocationTarget,\n    ConstructorTarget,\n    MemberDeclarationInstance,\n    MemberDeclarationStatic,\n    MemberInvocationInstance,\n    MemberInvocationStatic,\n    TypeAnnotation,\n""",
)

replace_once(
    "crates/dartscope-parse/src/lib.rs",
    """mod lexical_writes;\nmod namespace;\n""",
    """mod lexical_writes;\nmod member_references;\nmod namespace;\n""",
)

replace_once(
    "crates/dartscope-parse/src/analysis.rs",
    """use crate::lexical_writes::{collect_lexical_update_references, collect_lexical_write_references};\nuse crate::namespace::{directive_uri, extract_namespace_directives};\n""",
    """use crate::lexical_writes::{collect_lexical_update_references, collect_lexical_write_references};\nuse crate::member_references::collect_method_references;\nuse crate::namespace::{directive_uri, extract_namespace_directives};\n""",
)

replace_once(
    "crates/dartscope-parse/src/analysis.rs",
    """    references.extend(lexical_updates);\n    sort_identifier_references(&mut references);\n""",
    """    references.extend(lexical_updates);\n    references.extend(collect_method_references(\n        &source,\n        &lexical.code,\n        &file,\n        &bindings,\n    ));\n    sort_identifier_references(&mut references);\n""",
)

replace_once(
    "crates/dartscope-parse/src/analysis.rs",
    """        file_references.extend(lexical_updates);\n        references.extend(file_references);\n""",
    """        file_references.extend(lexical_updates);\n        file_references.extend(collect_method_references(\n            source,\n            &lexical.code,\n            file,\n            &file_bindings,\n        ));\n        references.extend(file_references);\n""",
)

Path("crates/dartscope-parse/src/member_references.rs").write_text(
    r'''use dartscope_core::{
    Confidence, DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, DartInvocation, DartLexicalBinding, SourceSpan,
};

use crate::source_lines::span_for_byte_range;

pub(crate) fn collect_method_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
) -> Vec<DartIdentifierReference> {
    let mut references = method_declaration_references(source, masked_source, analysis);
    for invocation in &analysis.invocations {
        let Some(reference) = method_invocation_reference(
            source,
            masked_source,
            analysis,
            bindings,
            invocation,
        ) else {
            continue;
        };
        references.push(reference);
    }
    references.sort_by(|left, right| {
        (
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
    references.dedup();
    references
}

fn method_declaration_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DartDeclarationKind::Method)
        .filter_map(|declaration| {
            let owner_symbol_id = declaration.parent_symbol_id.clone()?;
            let (name_start, name_end) = declaration_name_range(masked_source, declaration)?;
            let kind = if declaration_is_static(masked_source, declaration, name_start) {
                DartIdentifierReferenceKind::MemberDeclarationStatic
            } else {
                DartIdentifierReferenceKind::MemberDeclarationInstance
            };
            Some(DartIdentifierReference {
                source_path: analysis.path.clone(),
                name: declaration.name.clone(),
                prefix: Some(owner_symbol_id),
                kind,
                confidence: Confidence::High,
                enclosing_symbol_id: None,
                span: span_for_byte_range(source, name_start, name_end),
            })
        })
        .collect()
}

fn method_invocation_reference(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    bindings: &[DartLexicalBinding],
    invocation: &DartInvocation,
) -> Option<DartIdentifierReference> {
    if has_constructor_keyword(masked_source, invocation.span.byte_start) {
        return None;
    }
    let segments = invocation.target.split('.').collect::<Vec<_>>();
    let (kind, owner, member, confidence) = match segments.as_slice() {
        ["this", member] => (
            DartIdentifierReferenceKind::MemberInvocationInstance,
            enclosing_owner_symbol_id(analysis, invocation)?.to_string(),
            *member,
            Confidence::High,
        ),
        [owner, member]
            if looks_like_type_name(owner)
                && !binding_is_visible(bindings, owner, invocation.span.byte_start) =>
        {
            (
                DartIdentifierReferenceKind::MemberInvocationStatic,
                (*owner).to_string(),
                *member,
                Confidence::Medium,
            )
        }
        [import_prefix, owner, member]
            if looks_like_type_name(owner)
                && analysis
                    .imports
                    .iter()
                    .any(|import| import.prefix.as_deref() == Some(*import_prefix)) =>
        {
            (
                DartIdentifierReferenceKind::MemberInvocationStatic,
                format!("{import_prefix}.{owner}"),
                *member,
                Confidence::High,
            )
        }
        _ => return None,
    };
    let (member_start, member_end) = invocation_member_range(masked_source, invocation, member)?;
    Some(DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: member.to_string(),
        prefix: Some(owner),
        kind,
        confidence,
        enclosing_symbol_id: invocation.enclosing_symbol_id.clone(),
        span: span_for_byte_range(source, member_start, member_end),
    })
}

fn enclosing_owner_symbol_id<'a>(
    analysis: &'a DartFileAnalysis,
    invocation: &DartInvocation,
) -> Option<&'a str> {
    let callable_id = invocation.enclosing_symbol_id.as_deref()?;
    let callable = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.symbol_id.as_deref() == Some(callable_id))?;
    let owner_id = callable.parent_symbol_id.as_deref()?;
    analysis
        .declarations
        .iter()
        .find(|declaration| {
            declaration.symbol_id.as_deref() == Some(owner_id)
                && is_member_owner_kind(declaration.kind)
        })?;
    Some(owner_id)
}

fn declaration_name_range(
    masked_source: &str,
    declaration: &DartDeclaration,
) -> Option<(usize, usize)> {
    let span = declaration
        .declaration_span
        .as_ref()
        .unwrap_or(&declaration.span);
    let header_end = declaration_header_end(masked_source, span);
    let bytes = masked_source.as_bytes();
    let mut at = span.byte_start;
    let mut found = None;
    while at < header_end.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let end = identifier_end(bytes, at);
        if masked_source.get(at..end) == Some(declaration.name.as_str()) {
            found = Some((at, end));
        }
        at = end;
    }
    found
}

fn declaration_is_static(
    masked_source: &str,
    declaration: &DartDeclaration,
    name_start: usize,
) -> bool {
    let span = declaration
        .declaration_span
        .as_ref()
        .unwrap_or(&declaration.span);
    let bytes = masked_source.as_bytes();
    let mut at = span.byte_start;
    while at < name_start.min(bytes.len()) {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let end = identifier_end(bytes, at);
        if masked_source.get(at..end) == Some("static") {
            return true;
        }
        at = end;
    }
    false
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut at = span.byte_start;
    while at < span.byte_end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => return at,
            b'=' if parens == 0 && brackets == 0 && bytes.get(at + 1) == Some(&b'>') => {
                return at;
            }
            _ => {}
        }
        at += 1;
    }
    span.byte_end.min(bytes.len())
}

fn invocation_member_range(
    masked_source: &str,
    invocation: &DartInvocation,
    member: &str,
) -> Option<(usize, usize)> {
    let start = invocation.span.byte_start;
    let end = invocation.span.byte_end.min(masked_source.len());
    let expression = masked_source.get(start..end)?;
    let header_end = expression.find('(').unwrap_or(expression.len());
    let header = expression.get(..header_end)?;
    let relative = header.rfind(member)?;
    let member_start = start + relative;
    let member_end = member_start + member.len();
    let bytes = masked_source.as_bytes();
    if member_start > 0
        && bytes
            .get(member_start - 1)
            .is_some_and(|byte| is_identifier_continue(*byte))
    {
        return None;
    }
    if bytes
        .get(member_end)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        return None;
    }
    Some((member_start, member_end))
}

fn has_constructor_keyword(source: &str, start: usize) -> bool {
    let before = source.get(..start).unwrap_or_default().trim_end();
    let token = before
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .next()
        .unwrap_or_default();
    matches!(token, "new" | "const")
}

fn binding_is_visible(bindings: &[DartLexicalBinding], name: &str, at: usize) -> bool {
    bindings.iter().any(|binding| {
        binding.name == name
            && binding.scope_span.byte_start <= at
            && at < binding.scope_span.byte_end
    })
}

fn looks_like_type_name(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_uppercase)
}

fn is_member_owner_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

fn identifier_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
''',
    encoding="utf-8",
)

replace_once(
    "crates/dartscope-index/src/references.rs",
    """                DartIdentifierReferenceKind::VariableRead\n                    | DartIdentifierReferenceKind::VariableWrite\n""",
    """                DartIdentifierReferenceKind::VariableRead\n                    | DartIdentifierReferenceKind::VariableWrite\n                    | DartIdentifierReferenceKind::MemberDeclarationInstance\n                    | DartIdentifierReferenceKind::MemberDeclarationStatic\n                    | DartIdentifierReferenceKind::MemberInvocationInstance\n                    | DartIdentifierReferenceKind::MemberInvocationStatic\n""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """struct ResolvedReference {\n    reference: DartIdentifierReference,\n    status: DartDefinitionResolutionStatus,\n    targets: Vec<DartDefinitionTarget>,\n    external_uris: Vec<String>,\n}\n\n""",
    """struct ResolvedReference {\n    reference: DartIdentifierReference,\n    status: DartDefinitionResolutionStatus,\n    targets: Vec<DartDefinitionTarget>,\n    external_uris: Vec<String>,\n}\n\n#[derive(Debug, Clone, Eq, PartialEq)]\nstruct IndexedMethod {\n    owner_symbol_id: String,\n    is_static: bool,\n    candidate: DartSymbolCandidate,\n}\n\n#[derive(Debug, Clone, Default)]\nstruct MemberIndex {\n    methods: Vec<IndexedMethod>,\n}\n\nimpl MemberIndex {\n    fn new(analysis: &DartProjectReferenceAnalysis) -> Self {\n        let mut methods = analysis\n            .references\n            .iter()\n            .filter_map(|reference| {\n                let is_static = match reference.kind {\n                    DartIdentifierReferenceKind::MemberDeclarationInstance => false,\n                    DartIdentifierReferenceKind::MemberDeclarationStatic => true,\n                    _ => return None,\n                };\n                let owner_symbol_id = reference.prefix.clone()?;\n                let file = analysis\n                    .project\n                    .files\n                    .iter()\n                    .find(|file| file.path == reference.source_path)?;\n                let declaration = file.declarations.iter().find(|declaration| {\n                    declaration.kind == DartDeclarationKind::Method\n                        && declaration.name == reference.name\n                        && declaration.parent_symbol_id.as_deref() == Some(owner_symbol_id.as_str())\n                        && declaration_span_contains(declaration, &reference.span)\n                })?;\n                Some(IndexedMethod {\n                    owner_symbol_id,\n                    is_static,\n                    candidate: declaration_candidate(\n                        file.path.as_str(),\n                        declaration,\n                        DartSymbolResolutionBasis::SameFile,\n                    ),\n                })\n            })\n            .collect::<Vec<_>>();\n        methods.sort_by(|left, right| {\n            (\n                &left.owner_symbol_id,\n                left.is_static,\n                &left.candidate.declaration_path,\n                left.candidate.declaration_span.byte_start,\n                &left.candidate.name,\n            )\n                .cmp(&(\n                    &right.owner_symbol_id,\n                    right.is_static,\n                    &right.candidate.declaration_path,\n                    right.candidate.declaration_span.byte_start,\n                    &right.candidate.name,\n                ))\n        });\n        methods.dedup();\n        Self { methods }\n    }\n}\n\n""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """        let mut references = analysis.references.clone();\n        suppress_redundant_constructor_invocations(&mut references);\n""",
    """        let member_index = MemberIndex::new(analysis);\n        let mut references = analysis.references.clone();\n        suppress_redundant_constructor_invocations(&mut references);\n""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """            .map(|reference| resolve_reference(analysis, &namespace, &uri_graph, reference))\n""",
    """            .map(|reference| {\n                resolve_reference(analysis, &namespace, &uri_graph, &member_index, reference)\n            })\n""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """                        resolution.status == DartDefinitionResolutionStatus::Resolved\n                            && resolution.targets.len() == 1\n""",
    """                        resolution.status == DartDefinitionResolutionStatus::Resolved\n                            && !is_member_declaration_kind(resolution.reference.kind)\n                            && resolution.targets.len() == 1\n""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """fn resolve_reference(\n    analysis: &DartProjectReferenceAnalysis,\n    namespace: &NamespaceResolver<'_, '_>,\n    uri_graph: &DartUriGraph,\n    reference: DartIdentifierReference,\n) -> ResolvedReference {\n""",
    """fn resolve_reference(\n    analysis: &DartProjectReferenceAnalysis,\n    namespace: &NamespaceResolver<'_, '_>,\n    uri_graph: &DartUriGraph,\n    member_index: &MemberIndex,\n    reference: DartIdentifierReference,\n) -> ResolvedReference {\n    if is_member_declaration_kind(reference.kind) {\n        return resolve_method_declaration_reference(member_index, reference);\n    }\n    if matches!(\n        reference.kind,\n        DartIdentifierReferenceKind::MemberInvocationInstance\n            | DartIdentifierReferenceKind::MemberInvocationStatic\n    ) {\n        return resolve_method_invocation_reference(\n            analysis,\n            namespace,\n            uri_graph,\n            member_index,\n            reference,\n        );\n    }\n""",
)

insert_before = "fn resolve_constructor_reference(\n"
path = Path("crates/dartscope-index/src/navigation.rs")
text = path.read_text(encoding="utf-8")
member_code = r'''fn resolve_method_declaration_reference(
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let is_static = reference.kind == DartIdentifierReferenceKind::MemberDeclarationStatic;
    let owner_symbol_id = reference.prefix.as_deref();
    let mut targets = member_index
        .methods
        .iter()
        .filter(|method| {
            Some(method.owner_symbol_id.as_str()) == owner_symbol_id
                && method.is_static == is_static
                && method.candidate.name == reference.name
                && method.candidate.declaration_path == reference.source_path
                && method.candidate.declaration_span.byte_start <= reference.span.byte_start
                && reference.span.byte_end <= method.candidate.declaration_span.byte_end
        })
        .map(|method| DartDefinitionTarget::Namespace(method.candidate.clone()))
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

fn resolve_method_invocation_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    member_index: &MemberIndex,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    match reference.kind {
        DartIdentifierReferenceKind::MemberInvocationInstance => {
            resolve_instance_method_reference(analysis, namespace, member_index, reference)
        }
        DartIdentifierReferenceKind::MemberInvocationStatic => resolve_static_method_reference(
            analysis,
            namespace,
            uri_graph,
            member_index,
            reference,
        ),
        _ => unreachable!("member invocation resolver received a non-member fact"),
    }
}

fn resolve_instance_method_reference(
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
    let refinements = owners
        .iter()
        .map(|owner| {
            refine_method_target(
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                false,
            )
        })
        .collect::<Vec<_>>();
    finish_method_resolution(reference, refinements, Vec::new())
}

fn resolve_static_method_reference(
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
    let refinements = resolution
        .candidates
        .iter()
        .map(|owner| {
            refine_method_target(
                member_index,
                namespace,
                &reference.source_path,
                owner,
                &reference.name,
                true,
            )
        })
        .collect::<Vec<_>>();
    if base_status == DartDefinitionResolutionStatus::Resolved {
        finish_method_resolution(reference, refinements, external_uris)
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
struct MethodRefinement {
    status: DartDefinitionResolutionStatus,
    targets: Vec<DartDefinitionTarget>,
}

fn refine_method_target(
    member_index: &MemberIndex,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    member_name: &str,
    is_static: bool,
) -> MethodRefinement {
    let Some(owner_symbol_id) = owner.symbol_id.as_deref() else {
        return missing_method_target(owner);
    };
    let mut exact = member_index
        .methods
        .iter()
        .filter(|method| {
            method.owner_symbol_id == owner_symbol_id
                && method.is_static == is_static
                && method.candidate.name == member_name
        })
        .map(|method| {
            let mut candidate = method.candidate.clone();
            candidate.basis = owner.basis;
            candidate
        })
        .collect::<Vec<_>>();
    if exact.is_empty() {
        return missing_method_target(owner);
    }
    let visible = !member_name.starts_with('_')
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
            &left.symbol_id,
        )
            .cmp(&(
                &right.declaration_path,
                right.declaration_span.byte_start,
                &right.name,
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
    MethodRefinement {
        status,
        targets: exact
            .into_iter()
            .map(DartDefinitionTarget::Namespace)
            .collect(),
    }
}

fn missing_method_target(owner: &DartSymbolCandidate) -> MethodRefinement {
    MethodRefinement {
        status: DartDefinitionResolutionStatus::Missing,
        targets: vec![DartDefinitionTarget::Namespace(owner.clone())],
    }
}

fn finish_method_resolution(
    reference: DartIdentifierReference,
    refinements: Vec<MethodRefinement>,
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

fn static_member_owner(reference: &DartIdentifierReference) -> Option<(Option<String>, String)> {
    let parts = reference.prefix.as_deref()?.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        [owner] if !owner.is_empty() => Some((None, (*owner).to_string())),
        [prefix, owner] if !prefix.is_empty() && !owner.is_empty() => {
            Some((Some((*prefix).to_string()), (*owner).to_string()))
        }
        _ => None,
    }
}

fn member_owner_candidates_by_symbol_id(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner_symbol_id: &str,
) -> Vec<DartSymbolCandidate> {
    let mut owners = Vec::new();
    for file in &analysis.project.files {
        for declaration in &file.declarations {
            if declaration.symbol_id.as_deref() != Some(owner_symbol_id)
                || !is_member_owner_kind(declaration.kind)
            {
                continue;
            }
            let basis = if file.path == source_path {
                DartSymbolResolutionBasis::SameFile
            } else if namespace.same_library(source_path, &file.path) {
                DartSymbolResolutionBasis::SameLibrary
            } else {
                DartSymbolResolutionBasis::NotVisible
            };
            owners.push(declaration_candidate(file.path.as_str(), declaration, basis));
        }
    }
    owners
}

fn external_member_owner_uris(
    analysis: &DartProjectReferenceAnalysis,
    uri_graph: &DartUriGraph,
    reference: &DartIdentifierReference,
    owner_name: &str,
    import_prefix: Option<String>,
) -> Vec<String> {
    let mut owner_reference = reference.clone();
    owner_reference.name = owner_name.to_string();
    owner_reference.prefix = import_prefix;
    owner_reference.kind = DartIdentifierReferenceKind::InvocationTarget;
    external_namespace_uris(analysis, uri_graph, &owner_reference)
}

fn declaration_candidate(
    path: &str,
    declaration: &DartDeclaration,
    basis: DartSymbolResolutionBasis,
) -> DartSymbolCandidate {
    DartSymbolCandidate {
        name: declaration.name.clone(),
        kind: declaration.kind,
        symbol_id: declaration.symbol_id.clone(),
        declaration_path: path.to_string(),
        declaration_span: declaration
            .declaration_span
            .clone()
            .unwrap_or_else(|| declaration.span.clone()),
        basis,
    }
}

fn declaration_span_contains(declaration: &DartDeclaration, span: &dartscope_core::SourceSpan) -> bool {
    let declaration_span = declaration
        .declaration_span
        .as_ref()
        .unwrap_or(&declaration.span);
    declaration_span.byte_start <= span.byte_start && span.byte_end <= declaration_span.byte_end
}

fn is_member_owner_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

fn is_member_declaration_kind(kind: DartIdentifierReferenceKind) -> bool {
    matches!(
        kind,
        DartIdentifierReferenceKind::MemberDeclarationInstance
            | DartIdentifierReferenceKind::MemberDeclarationStatic
    )
}

'''
if member_code not in text:
    if text.count(insert_before) != 1:
        raise SystemExit("navigation.rs: constructor resolver insertion point missing")
    path.write_text(text.replace(insert_before, member_code + insert_before, 1), encoding="utf-8")

Path("crates/dartscope-index/tests/navigation_methods.rs").write_text(
    r'''use dartscope_core::{
    DartCompilationEnvironment, DartDeclarationKind, DartFileInput, DartProjectInput,
    DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartIndexOptions, DartWorkspaceResolutionContext,
};
use dartscope_parse::analyze_project_with_references;

const TYPES: &str = r#"
class Service {
  static void build() {}
  static void _hidden() {}

  void exercise() {
    this.work();
    this._private();
    this.missing();
  }

  void work() {}
  void _private() {}
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types;

void run() {
  types.Service.build();
  types.Service._hidden();
  types.Service.missing();
}
"#;

#[test]
fn resolves_exact_static_and_this_method_targets() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let build = occurrence(CLIENT, "Service.build", "build");
    let hidden = occurrence(CLIENT, "Service._hidden", "_hidden");
    let missing = occurrence(CLIENT, "Service.missing", "missing");
    let work = occurrence(TYPES, "this.work", "work");
    let private = occurrence(TYPES, "this._private", "_private");
    let local_missing = occurrence(TYPES, "this.missing", "missing");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/client.dart", build),
        DartDefinitionQuery::new("lib/client.dart", hidden),
        DartDefinitionQuery::new("lib/client.dart", missing),
        DartDefinitionQuery::new("lib/types.dart", work),
        DartDefinitionQuery::new("lib/types.dart", private),
        DartDefinitionQuery::new("lib/types.dart", local_missing),
    ]);

    assert_method_target(
        resolution_at(&batch.resolutions, "lib/client.dart", build),
        DartDefinitionResolutionStatus::Resolved,
        "build",
        "lib/types.dart",
    );
    let hidden = resolution_at(&batch.resolutions, "lib/client.dart", hidden);
    assert_method_target(
        hidden,
        DartDefinitionResolutionStatus::NotVisible,
        "_hidden",
        "lib/types.dart",
    );
    assert!(matches!(
        &hidden.targets[0],
        DartDefinitionTarget::Namespace(candidate)
            if candidate.basis == DartSymbolResolutionBasis::NotVisible
    ));
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/client.dart", missing),
        "Service",
    );
    assert_method_target(
        resolution_at(&batch.resolutions, "lib/types.dart", work),
        DartDefinitionResolutionStatus::Resolved,
        "work",
        "lib/types.dart",
    );
    assert_method_target(
        resolution_at(&batch.resolutions, "lib/types.dart", private),
        DartDefinitionResolutionStatus::Resolved,
        "_private",
        "lib/types.dart",
    );
    assert_owner_fallback(
        resolution_at(&batch.resolutions, "lib/types.dart", local_missing),
        "Service",
    );

    let build_target = resolution_at(&batch.resolutions, "lib/client.dart", build).targets[0].clone();
    let references = context.find_references(&[build_target.clone()]);
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].target, build_target);
    assert_eq!(references.results[0].references.len(), 1);
    assert_eq!(references.results[0].references[0].span.byte_start, build);
}

const FIRST: &str = r#"
class Shared {
  static void open() {}
}
"#;
const SECOND: &str = r#"
class Shared {
  static void open() {}
}
"#;
const SERVICE_STUB: &str = r#"
class ConditionalService {
  static void open() {}
}
"#;
const SERVICE_IO: &str = r#"
class ConditionalService {
  static void open() {}
}
"#;
const EVIDENCE_CLIENT: &str = r#"
import 'first.dart';
import 'second.dart';
import 'service_stub.dart' if (dart.library.io) 'service_io.dart';
import 'package:widgets/api.dart' as widgets;

void run() {
  Shared.open();
  ConditionalService.open();
  widgets.Widget.open();
}
"#;

#[test]
fn preserves_ambiguous_conditional_and_external_method_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/first.dart", FIRST),
            DartFileInput::new("lib/second.dart", SECOND),
            DartFileInput::new("lib/service_stub.dart", SERVICE_STUB),
            DartFileInput::new("lib/service_io.dart", SERVICE_IO),
            DartFileInput::new("lib/client.dart", EVIDENCE_CLIENT),
        ],
        vec![],
    ));
    let shared = occurrence(EVIDENCE_CLIENT, "Shared.open", "open");
    let conditional = occurrence(EVIDENCE_CLIENT, "ConditionalService.open", "open");
    let external = occurrence(EVIDENCE_CLIENT, "Widget.open", "open");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", shared),
        DartDefinitionQuery::new("lib/client.dart", conditional),
        DartDefinitionQuery::new("lib/client.dart", external),
    ];
    let unresolved = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&queries);

    let shared = resolution_at(&unresolved.resolutions, "lib/client.dart", shared);
    assert_eq!(shared.status, DartDefinitionResolutionStatus::Ambiguous);
    assert_eq!(shared.targets.len(), 2);
    assert!(shared.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Method && candidate.name == "open"
    )));

    let conditional = resolution_at(&unresolved.resolutions, "lib/client.dart", conditional);
    assert_eq!(
        conditional.status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(conditional.targets.len(), 2);

    let external = resolution_at(&unresolved.resolutions, "lib/client.dart", external);
    assert_eq!(
        external.status,
        DartDefinitionResolutionStatus::ExternalUnindexed
    );
    assert_eq!(external.external_uris, ["package:widgets/api.dart"]);
    assert!(external.targets.is_empty());

    let options = DartIndexOptions::default().with_compilation_environment(
        DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
    );
    let resolved = DartWorkspaceResolutionContext::with_options(&analysis, &options)
        .find_definitions(&[DartDefinitionQuery::new("lib/client.dart", conditional.query.byte_offset)]);
    assert_method_target(
        &resolved.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "open",
        "lib/service_io.dart",
    );
}

const OWNER: &str = r#"
library sample;
part 'part.dart';

void run() {
  PartService.open();
}
"#;
const PART: &str = r#"
part of 'owner.dart';

class PartService {
  static void open() {}
}
"#;

#[test]
fn resolves_static_methods_declared_in_a_part_library() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let open = occurrence(OWNER, "PartService.open", "open");
    let batch = DartWorkspaceResolutionContext::new(&analysis)
        .find_definitions(&[DartDefinitionQuery::new("lib/owner.dart", open)]);
    assert_method_target(
        &batch.resolutions[0],
        DartDefinitionResolutionStatus::Resolved,
        "open",
        "lib/part.dart",
    );
}

fn resolution_at<'a>(
    resolutions: &'a [DartDefinitionResolution],
    path: &str,
    byte_offset: usize,
) -> &'a DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| {
            resolution.query.source_path == path && resolution.query.byte_offset == byte_offset
        })
        .unwrap_or_else(|| panic!("missing definition result at {path}:{byte_offset}"))
}

fn assert_method_target(
    resolution: &DartDefinitionResolution,
    status: DartDefinitionResolutionStatus,
    name: &str,
    path: &str,
) {
    assert_eq!(resolution.status, status);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, DartDeclarationKind::Method);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, path);
        }
        target => panic!("unexpected method target: {target:?}"),
    }
}

fn assert_owner_fallback(resolution: &DartDefinitionResolution, owner: &str) {
    assert_eq!(resolution.status, DartDefinitionResolutionStatus::Missing);
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.name, owner);
            assert_eq!(candidate.kind, DartDeclarationKind::Class);
        }
        target => panic!("unexpected owner target: {target:?}"),
    }
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}
''',
    encoding="utf-8",
)
