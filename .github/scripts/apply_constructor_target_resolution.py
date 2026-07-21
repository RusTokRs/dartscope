from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:80]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


TEST_PATH = Path("crates/dartscope-index/tests/navigation_constructors.rs")
if TEST_PATH.exists():
    raise SystemExit(0)

replace_once(
    "crates/dartscope-index/src/namespace.rs",
    """use dartscope_core::{
    DartDeclaration, DartFileAnalysis, DartNamespaceCombinatorKind, DartPartLinkAnalysis,
    DartPartLinkStatus, DartProjectAnalysis, DartSymbolCandidate, DartSymbolQuery,
    DartSymbolResolution, DartSymbolResolutionBasis, DartSymbolResolutionStatus, DartUriGraph,
    DartUriReferenceKind, DartUriResolution, SourceSpan,
};
""",
    """use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartNamespaceCombinatorKind,
    DartPartLinkAnalysis, DartPartLinkStatus, DartProjectAnalysis, DartSymbolCandidate,
    DartSymbolQuery, DartSymbolResolution, DartSymbolResolutionBasis, DartSymbolResolutionStatus,
    DartUriGraph, DartUriReferenceKind, DartUriResolution, SourceSpan,
};
""",
)

replace_once(
    "crates/dartscope-index/src/namespace.rs",
    """    pub(crate) fn resolve(
        &self,
""",
    """    pub(crate) fn same_library(&self, left: &str, right: &str) -> bool {
        self.library_membership.same_library(left, right)
    }

    pub(crate) fn resolve(
        &self,
""",
)

replace_once(
    "crates/dartscope-index/src/namespace.rs",
    """pub(crate) fn resolve_symbol_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    let declarations = collect_declarations(project, query.name.as_str());
    let candidates: Vec<_> = declarations
        .iter()
        .map(|location| NamespaceCandidate {
            path: location.path,
            byte_start: location.declaration.span.byte_start,
        })
        .collect();
    let resolution = resolver.resolve(
        query.source_path.as_str(),
        query.name.as_str(),
        query.prefix.as_deref(),
        &candidates,
    );
    let mut outputs: Vec<_> = resolution
        .candidates
        .iter()
        .map(|candidate| candidate_output(&declarations[candidate.index], candidate.basis))
        .collect();
    sort_candidates(&mut outputs);
    DartSymbolResolution {
        query,
        status: resolution.status,
        candidates: outputs,
    }
}
""",
    """pub(crate) fn resolve_symbol_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    resolve_symbol_with_resolver_filter(project, query, resolver, |_| true)
}

pub(crate) fn resolve_constructible_type_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    resolve_symbol_with_resolver_filter(project, query, resolver, |kind| {
        matches!(
            kind,
            DartDeclarationKind::Class | DartDeclarationKind::ExtensionType
        )
    })
}

fn resolve_symbol_with_resolver_filter(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
    allowed_kind: impl Fn(DartDeclarationKind) -> bool,
) -> DartSymbolResolution {
    let declarations = collect_declarations(project, query.name.as_str(), &allowed_kind);
    let candidates: Vec<_> = declarations
        .iter()
        .map(|location| NamespaceCandidate {
            path: location.path,
            byte_start: location.declaration.span.byte_start,
        })
        .collect();
    let resolution = resolver.resolve(
        query.source_path.as_str(),
        query.name.as_str(),
        query.prefix.as_deref(),
        &candidates,
    );
    let mut outputs: Vec<_> = resolution
        .candidates
        .iter()
        .map(|candidate| candidate_output(&declarations[candidate.index], candidate.basis))
        .collect();
    sort_candidates(&mut outputs);
    DartSymbolResolution {
        query,
        status: resolution.status,
        candidates: outputs,
    }
}
""",
)

replace_once(
    "crates/dartscope-index/src/namespace.rs",
    """fn collect_declarations<'a>(
    project: &'a DartProjectAnalysis,
    name: &str,
) -> Vec<DeclarationLocation<'a>> {
""",
    """fn collect_declarations<'a>(
    project: &'a DartProjectAnalysis,
    name: &str,
    allowed_kind: &impl Fn(DartDeclarationKind) -> bool,
) -> Vec<DeclarationLocation<'a>> {
""",
)

replace_once(
    "crates/dartscope-index/src/namespace.rs",
    """            if declaration.parent_symbol_id.is_none() && declaration.name == name {
""",
    """            if declaration.parent_symbol_id.is_none()
                && declaration.name == name
                && allowed_kind(declaration.kind)
            {
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """use dartscope_core::{
    DartIdentifierReference, DartIdentifierReferenceKind, DartLexicalBinding,
    DartLexicalBindingQuery, DartLexicalBindingResolutionStatus, DartNamespaceCombinatorKind,
    DartProjectReferenceAnalysis, DartSymbolCandidate, DartSymbolQuery, DartSymbolResolutionStatus,
    DartUriGraph, DartUriReferenceKind, DartUriResolution, normalize_path,
};
""",
    """use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartIdentifierReference, DartIdentifierReferenceKind,
    DartLexicalBinding, DartLexicalBindingQuery, DartLexicalBindingResolutionStatus,
    DartNamespaceCombinatorKind, DartProjectReferenceAnalysis, DartSymbolCandidate, DartSymbolQuery,
    DartSymbolResolutionBasis, DartSymbolResolutionStatus, DartUriGraph, DartUriReferenceKind,
    DartUriResolution, normalize_path,
};
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """use crate::namespace::{NamespaceResolver, resolve_symbol_with_resolver};
""",
    """use crate::namespace::{
    NamespaceResolver, resolve_constructible_type_with_resolver, resolve_symbol_with_resolver,
};
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """    if matches!(
        reference.kind,
        DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
    ) {
        return resolve_lexical_reference(analysis, reference);
    }

    let query = DartSymbolQuery {
""",
    """    if matches!(
        reference.kind,
        DartIdentifierReferenceKind::VariableRead | DartIdentifierReferenceKind::VariableWrite
    ) {
        return resolve_lexical_reference(analysis, reference);
    }
    if reference.kind == DartIdentifierReferenceKind::ConstructorTarget {
        return resolve_constructor_reference(analysis, namespace, uri_graph, reference);
    }

    let query = DartSymbolQuery {
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """    let status = match resolution.status {
        DartSymbolResolutionStatus::Resolved => DartDefinitionResolutionStatus::Resolved,
        DartSymbolResolutionStatus::Missing if !external_uris.is_empty() => {
            DartDefinitionResolutionStatus::ExternalUnindexed
        }
        DartSymbolResolutionStatus::Missing => DartDefinitionResolutionStatus::Missing,
        DartSymbolResolutionStatus::Ambiguous => DartDefinitionResolutionStatus::Ambiguous,
        DartSymbolResolutionStatus::NotVisible => DartDefinitionResolutionStatus::NotVisible,
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => {
            DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
        }
        DartSymbolResolutionStatus::SourceFileMissing => {
            DartDefinitionResolutionStatus::SourceFileMissing
        }
    };
""",
    """    let status = definition_status(resolution.status, !external_uris.is_empty());
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """fn resolve_lexical_reference(
""",
    """fn resolve_constructor_reference(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    uri_graph: &DartUriGraph,
    reference: DartIdentifierReference,
) -> ResolvedReference {
    let query = DartSymbolQuery {
        source_path: reference.source_path.clone(),
        name: reference.name.clone(),
        prefix: reference.prefix.clone(),
    };
    let resolution =
        resolve_constructible_type_with_resolver(&analysis.project, query, namespace);
    let external_uris = external_namespace_uris(analysis, uri_graph, &reference);
    let base_status = definition_status(resolution.status, !external_uris.is_empty());
    let constructor_name = constructor_declaration_name(analysis, &reference);
    let refinements = resolution
        .candidates
        .iter()
        .map(|owner| {
            refine_constructor_target(
                analysis,
                namespace,
                &reference.source_path,
                owner,
                &constructor_name,
            )
        })
        .collect::<Vec<_>>();
    let mut targets = refinements
        .iter()
        .flat_map(|refinement| refinement.targets.iter().cloned())
        .collect::<Vec<_>>();
    targets.sort_by(compare_targets);
    targets.dedup_by(|left, right| same_target(left, right));
    let status = if base_status == DartDefinitionResolutionStatus::Resolved {
        let statuses = refinements
            .iter()
            .map(|refinement| refinement.status)
            .collect::<Vec<_>>();
        combine_statuses(&statuses, targets.len())
    } else {
        base_status
    };
    ResolvedReference {
        reference,
        status,
        targets,
        external_uris,
    }
}

#[derive(Debug)]
struct ConstructorRefinement {
    status: DartDefinitionResolutionStatus,
    targets: Vec<DartDefinitionTarget>,
}

fn refine_constructor_target(
    analysis: &DartProjectReferenceAnalysis,
    namespace: &NamespaceResolver<'_, '_>,
    source_path: &str,
    owner: &DartSymbolCandidate,
    constructor_name: &str,
) -> ConstructorRefinement {
    let Some(owner_symbol_id) = owner.symbol_id.as_deref() else {
        return fallback_constructor_target(owner, constructor_name, false);
    };
    let mut declared_count = 0_usize;
    let mut exact = Vec::new();
    for file in &analysis.project.files {
        for declaration in &file.declarations {
            if declaration.kind != DartDeclarationKind::Constructor
                || declaration.parent_symbol_id.as_deref() != Some(owner_symbol_id)
            {
                continue;
            }
            declared_count += 1;
            if declaration.name == constructor_name {
                exact.push(constructor_candidate(file.path.as_str(), declaration, owner.basis));
            }
        }
    }
    if exact.is_empty() {
        return fallback_constructor_target(owner, constructor_name, declared_count > 0);
    }

    let is_private = constructor_member_name(owner, constructor_name)
        .is_some_and(|name| name.starts_with('_'));
    let visible = !is_private || namespace.same_library(source_path, &owner.declaration_path);
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
    ConstructorRefinement {
        status,
        targets: exact
            .into_iter()
            .map(DartDefinitionTarget::Namespace)
            .collect(),
    }
}

fn fallback_constructor_target(
    owner: &DartSymbolCandidate,
    constructor_name: &str,
    has_declared_constructor: bool,
) -> ConstructorRefinement {
    let implicit_unnamed = constructor_name == owner.name && !has_declared_constructor;
    ConstructorRefinement {
        status: if implicit_unnamed {
            DartDefinitionResolutionStatus::Resolved
        } else {
            DartDefinitionResolutionStatus::Missing
        },
        targets: vec![DartDefinitionTarget::Namespace(owner.clone())],
    }
}

fn constructor_candidate(
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

fn constructor_member_name<'a>(
    owner: &DartSymbolCandidate,
    constructor_name: &'a str,
) -> Option<&'a str> {
    constructor_name
        .strip_prefix(owner.name.as_str())?
        .strip_prefix('.')
}

fn constructor_declaration_name(
    analysis: &DartProjectReferenceAnalysis,
    reference: &DartIdentifierReference,
) -> String {
    let Some(file) = analysis
        .project
        .files
        .iter()
        .find(|file| file.path == reference.source_path)
    else {
        return reference.name.clone();
    };
    file.invocations
        .iter()
        .filter(|invocation| {
            invocation.span.byte_start <= reference.span.byte_start
                && reference.span.byte_end <= invocation.span.byte_end
        })
        .filter_map(|invocation| {
            constructor_name_from_target(&invocation.target, reference).map(|name| {
                (
                    invocation.span.byte_end - invocation.span.byte_start,
                    invocation.span.byte_start,
                    name,
                )
            })
        })
        .min_by_key(|(length, start, _)| (*length, *start))
        .map(|(_, _, name)| name)
        .unwrap_or_else(|| reference.name.clone())
}

fn constructor_name_from_target(
    target: &str,
    reference: &DartIdentifierReference,
) -> Option<String> {
    let segments = target.split('.').collect::<Vec<_>>();
    let type_index = if let Some(prefix) = reference.prefix.as_deref() {
        (segments.first().copied() == Some(prefix)).then_some(1_usize)?
    } else {
        0
    };
    if segments.get(type_index).copied() != Some(reference.name.as_str())
        || segments.len() > type_index + 2
    {
        return None;
    }
    match segments.get(type_index + 1).copied() {
        None | Some("new") => Some(reference.name.clone()),
        Some(member) => Some(format!("{}.{member}", reference.name)),
    }
}

fn definition_status(
    status: DartSymbolResolutionStatus,
    has_external_uris: bool,
) -> DartDefinitionResolutionStatus {
    match status {
        DartSymbolResolutionStatus::Resolved => DartDefinitionResolutionStatus::Resolved,
        DartSymbolResolutionStatus::Missing if has_external_uris => {
            DartDefinitionResolutionStatus::ExternalUnindexed
        }
        DartSymbolResolutionStatus::Missing => DartDefinitionResolutionStatus::Missing,
        DartSymbolResolutionStatus::Ambiguous => DartDefinitionResolutionStatus::Ambiguous,
        DartSymbolResolutionStatus::NotVisible => DartDefinitionResolutionStatus::NotVisible,
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => {
            DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
        }
        DartSymbolResolutionStatus::SourceFileMissing => {
            DartDefinitionResolutionStatus::SourceFileMissing
        }
    }
}

fn resolve_lexical_reference(
""",
)

TEST_PATH.write_text(
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
class Imported {
  Imported();
  Imported.named();
  Imported._hidden();
}

class Implicit {}

class NamedOnly {
  NamedOnly.named();
}
"#;

const CLIENT: &str = r#"
import 'types.dart' as types show Imported, Implicit, NamedOnly;

void run() {
  new types.Imported();
  new types.Imported.named();
  new types.Imported._hidden();
  new types.Imported.missing();
  new types.Implicit();
  new types.NamedOnly();
}
"#;

#[test]
fn resolves_exact_prefixed_constructors_and_preserves_owner_fallback_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/types.dart", TYPES),
            DartFileInput::new("lib/client.dart", CLIENT),
        ],
        vec![],
    ));
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let offsets = CLIENT
        .match_indices("Imported")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let implicit = occurrence(CLIENT, "types.Implicit", "Implicit");
    let named_only = occurrence(CLIENT, "types.NamedOnly", "NamedOnly");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", offsets[1]),
        DartDefinitionQuery::new("lib/client.dart", offsets[2]),
        DartDefinitionQuery::new("lib/client.dart", offsets[3]),
        DartDefinitionQuery::new("lib/client.dart", offsets[4]),
        DartDefinitionQuery::new("lib/client.dart", implicit),
        DartDefinitionQuery::new("lib/client.dart", named_only),
    ];
    let batch = context.find_definitions(&queries);

    let unnamed = resolution_at(&batch.resolutions, offsets[1]);
    assert_eq!(unnamed.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(unnamed, "Imported", "lib/types.dart");

    let named = resolution_at(&batch.resolutions, offsets[2]);
    assert_eq!(named.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(named, "Imported.named", "lib/types.dart");

    let hidden = resolution_at(&batch.resolutions, offsets[3]);
    assert_eq!(hidden.status, DartDefinitionResolutionStatus::NotVisible);
    assert_constructor_target(hidden, "Imported._hidden", "lib/types.dart");
    match &hidden.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.basis, DartSymbolResolutionBasis::NotVisible);
        }
        target => panic!("unexpected hidden target: {target:?}"),
    }

    let missing = resolution_at(&batch.resolutions, offsets[4]);
    assert_eq!(missing.status, DartDefinitionResolutionStatus::Missing);
    assert_owner_target(missing, "Imported", DartDeclarationKind::Class);

    let implicit = resolution_at(&batch.resolutions, implicit);
    assert_eq!(implicit.status, DartDefinitionResolutionStatus::Resolved);
    assert_owner_target(implicit, "Implicit", DartDeclarationKind::Class);

    let named_only = resolution_at(&batch.resolutions, named_only);
    assert_eq!(named_only.status, DartDefinitionResolutionStatus::Missing);
    assert_owner_target(named_only, "NamedOnly", DartDeclarationKind::Class);

    let named_target = named.targets[0].clone();
    let references = context.find_references(&[named_target.clone()]);
    assert_eq!(references.results.len(), 1);
    assert_eq!(references.results[0].target, named_target);
    assert_eq!(references.results[0].references.len(), 1);
    assert_eq!(references.results[0].references[0].span.byte_start, offsets[2]);
}

const FIRST: &str = "class Shared { Shared.named(); }\n";
const SECOND: &str = "class Shared { Shared.named(); }\n";
const SERVICE_STUB: &str = "class Service { Service.named(); }\n";
const SERVICE_IO: &str = "class Service { Service.named(); }\n";
const ENV_CLIENT: &str = r#"
import 'first.dart';
import 'second.dart';
import 'service_stub.dart' if (dart.library.io) 'service_io.dart';
import 'package:widgets/api.dart';

void run() {
  new Shared.named();
  new Service.named();
  new Widget.named();
}
"#;

#[test]
fn preserves_ambiguity_conditional_and_external_constructor_evidence() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/first.dart", FIRST),
            DartFileInput::new("lib/second.dart", SECOND),
            DartFileInput::new("lib/service_stub.dart", SERVICE_STUB),
            DartFileInput::new("lib/service_io.dart", SERVICE_IO),
            DartFileInput::new("lib/client.dart", ENV_CLIENT),
        ],
        vec![],
    ));
    let shared = occurrence(ENV_CLIENT, "Shared.named", "Shared");
    let service = occurrence(ENV_CLIENT, "Service.named", "Service");
    let widget = occurrence(ENV_CLIENT, "Widget.named", "Widget");
    let queries = [
        DartDefinitionQuery::new("lib/client.dart", shared),
        DartDefinitionQuery::new("lib/client.dart", service),
        DartDefinitionQuery::new("lib/client.dart", widget),
    ];
    let unresolved = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&queries);

    let shared = resolution_at(&unresolved.resolutions, shared);
    assert_eq!(shared.status, DartDefinitionResolutionStatus::Ambiguous);
    assert_eq!(shared.targets.len(), 2);
    assert!(shared.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Constructor
                && candidate.name == "Shared.named"
    )));

    let conditional = resolution_at(&unresolved.resolutions, service);
    assert_eq!(
        conditional.status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(conditional.targets.len(), 2);
    assert!(conditional.targets.iter().all(|target| matches!(
        target,
        DartDefinitionTarget::Namespace(candidate)
            if candidate.kind == DartDeclarationKind::Constructor
                && candidate.name == "Service.named"
    )));

    let external = resolution_at(&unresolved.resolutions, widget);
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
        .find_definitions(&[DartDefinitionQuery::new("lib/client.dart", service)]);
    let service = resolution_at(&resolved.resolutions, service);
    assert_eq!(service.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(service, "Service.named", "lib/service_io.dart");
}

const OWNER: &str = r#"
library sample;
part 'part.dart';

void run() {
  new PartType.named();
  new PartType._hidden();
}
"#;
const PART: &str = r#"
part of 'owner.dart';

class PartType {
  PartType.named();
  PartType._hidden();
}
"#;

#[test]
fn resolves_public_and_private_constructors_across_one_part_library() {
    let analysis = analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/owner.dart", OWNER),
            DartFileInput::new("lib/part.dart", PART),
        ],
        vec![],
    ));
    let named = occurrence(OWNER, "PartType.named", "PartType");
    let hidden = occurrence(OWNER, "PartType._hidden", "PartType");
    let batch = DartWorkspaceResolutionContext::new(&analysis).find_definitions(&[
        DartDefinitionQuery::new("lib/owner.dart", named),
        DartDefinitionQuery::new("lib/owner.dart", hidden),
    ]);

    let named = resolution_at(&batch.resolutions, named);
    assert_eq!(named.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(named, "PartType.named", "lib/part.dart");

    let hidden = resolution_at(&batch.resolutions, hidden);
    assert_eq!(hidden.status, DartDefinitionResolutionStatus::Resolved);
    assert_constructor_target(hidden, "PartType._hidden", "lib/part.dart");
}

fn resolution_at(
    resolutions: &[DartDefinitionResolution],
    byte_offset: usize,
) -> &DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == byte_offset)
        .unwrap_or_else(|| panic!("missing definition result at {byte_offset}"))
}

fn assert_constructor_target(
    resolution: &DartDefinitionResolution,
    name: &str,
    path: &str,
) {
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, DartDeclarationKind::Constructor);
            assert_eq!(candidate.name, name);
            assert_eq!(candidate.declaration_path, path);
        }
        target => panic!("unexpected constructor target: {target:?}"),
    }
}

fn assert_owner_target(
    resolution: &DartDefinitionResolution,
    name: &str,
    kind: DartDeclarationKind,
) {
    assert_eq!(resolution.targets.len(), 1);
    match &resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.kind, kind);
            assert_eq!(candidate.name, name);
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
