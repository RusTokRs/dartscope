use dartscope_core::{DartFileInput, DartIdentifierReferenceKind, DartProjectInput};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartWorkspaceResolutionContext, find_definitions, find_references,
};
use dartscope_parse::analyze_project_with_references;

const A: &str = r#"
void helper() {}
"#;

const B: &str = r#"
import 'a.dart';

void consume(Object? input) {}

void run(int seed) {
  var local = seed;
  helper();
  local++;
  consume(local);
}
"#;

const C: &str = r#"
void other() {
  helper();
  absent();
}
"#;

const EXTERNAL: &str = r#"
import 'package:widgets/api.dart';
Widget make() => Widget();
"#;

const BASE: &str = r#"
void conditionalValue() {}
"#;

const IO: &str = r#"
void conditionalValue() {}
"#;

const CONDITIONAL: &str = r#"
import 'base.dart' if (dart.library.io) 'io.dart';
void use() { conditionalValue(); }
"#;

#[test]
fn resolves_namespace_lexical_and_explicit_unresolved_definition_batches() {
    let analysis = project();
    let local = occurrence(B, "local++;", "local");
    let helper = occurrence(B, "helper();", "helper");
    let not_visible = occurrence(C, "helper();", "helper");
    let missing = occurrence(C, "absent();", "absent");
    let external = nth_occurrence(EXTERNAL, "Widget", 1);
    let conditional = occurrence(CONDITIONAL, "conditionalValue();", "conditionalValue");
    let queries = [
        DartDefinitionQuery::new("missing.dart", 0),
        DartDefinitionQuery::new("lib/external.dart", external),
        DartDefinitionQuery::new("lib/b.dart", local),
        DartDefinitionQuery::new("lib/c.dart", missing),
        DartDefinitionQuery::new("lib/b.dart", helper),
        DartDefinitionQuery::new("lib/c.dart", not_visible),
        DartDefinitionQuery::new("lib/b.dart", occurrence(B, "void run", "void")),
        DartDefinitionQuery::new("lib/conditional.dart", conditional),
    ];
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let batch = context.find_definitions(&queries);
    assert_eq!(batch, find_definitions(&analysis, &queries));

    let helper_resolution = resolution_at(&batch.resolutions, "lib/b.dart", helper);
    assert_eq!(
        helper_resolution.status,
        DartDefinitionResolutionStatus::Resolved
    );
    assert_eq!(helper_resolution.targets.len(), 1);
    match &helper_resolution.targets[0] {
        DartDefinitionTarget::Namespace(candidate) => {
            assert_eq!(candidate.name, "helper");
            assert_eq!(candidate.declaration_path, "lib/a.dart");
        }
        target => panic!("unexpected helper target: {target:?}"),
    }

    let local_resolution = resolution_at(&batch.resolutions, "lib/b.dart", local);
    assert_eq!(
        local_resolution.status,
        DartDefinitionResolutionStatus::Resolved
    );
    assert_eq!(
        local_resolution
            .references
            .iter()
            .map(|reference| reference.kind)
            .collect::<Vec<_>>(),
        [
            DartIdentifierReferenceKind::VariableRead,
            DartIdentifierReferenceKind::VariableWrite,
        ]
    );
    assert_eq!(local_resolution.targets.len(), 1);
    match &local_resolution.targets[0] {
        DartDefinitionTarget::Lexical(binding) => {
            assert_eq!(binding.name, "local");
            assert!(binding.symbol_id.contains("/local_variable:local"));
        }
        target => panic!("unexpected local target: {target:?}"),
    }

    let not_visible_resolution = resolution_at(&batch.resolutions, "lib/c.dart", not_visible);
    assert_eq!(
        not_visible_resolution.status,
        DartDefinitionResolutionStatus::NotVisible
    );
    assert_eq!(not_visible_resolution.targets.len(), 1);

    let missing_resolution = resolution_at(&batch.resolutions, "lib/c.dart", missing);
    assert_eq!(
        missing_resolution.status,
        DartDefinitionResolutionStatus::Missing
    );
    assert!(missing_resolution.targets.is_empty());

    let external_resolution = resolution_at(&batch.resolutions, "lib/external.dart", external);
    assert_eq!(
        external_resolution.status,
        DartDefinitionResolutionStatus::ExternalUnindexed
    );
    assert_eq!(
        external_resolution.external_uris,
        ["package:widgets/api.dart"]
    );
    assert!(external_resolution.targets.is_empty());

    assert_eq!(
        resolution_at(&batch.resolutions, "lib/conditional.dart", conditional).status,
        DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
    );
    assert_eq!(
        resolution_at(
            &batch.resolutions,
            "lib/b.dart",
            occurrence(B, "void run", "void")
        )
        .status,
        DartDefinitionResolutionStatus::ReferenceMissing
    );
    assert_eq!(
        resolution_at(&batch.resolutions, "missing.dart", 0).status,
        DartDefinitionResolutionStatus::SourceFileMissing
    );
}

#[test]
fn finds_only_unambiguous_references_with_stable_target_and_fact_ordering() {
    let analysis = project();
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let helper = occurrence(B, "helper();", "helper");
    let local = occurrence(B, "local++;", "local");
    let definitions = context.find_definitions(&[
        DartDefinitionQuery::new("lib/b.dart", local),
        DartDefinitionQuery::new("lib/b.dart", helper),
    ]);
    let helper_target = resolution_at(&definitions.resolutions, "lib/b.dart", helper).targets[0].clone();
    let local_target = resolution_at(&definitions.resolutions, "lib/b.dart", local).targets[0].clone();
    let targets = [
        local_target.clone(),
        helper_target.clone(),
        helper_target.clone(),
    ];
    let references = context.find_references(&targets);
    assert_eq!(references, find_references(&analysis, &targets));
    assert_eq!(references.results.len(), 2);

    let helper_result = references
        .results
        .iter()
        .find(|result| result.target == helper_target)
        .expect("helper references");
    assert_eq!(helper_result.references.len(), 1);
    assert_eq!(helper_result.references[0].source_path, "lib/b.dart");
    assert_eq!(helper_result.references[0].span.byte_start, helper);

    let local_result = references
        .results
        .iter()
        .find(|result| result.target == local_target)
        .expect("local references");
    assert_eq!(
        local_result
            .references
            .iter()
            .map(|reference| (reference.span.byte_start, reference.kind))
            .collect::<Vec<_>>(),
        [
            (local, DartIdentifierReferenceKind::VariableRead),
            (local, DartIdentifierReferenceKind::VariableWrite),
            (
                occurrence(B, "consume(local)", "local"),
                DartIdentifierReferenceKind::VariableRead,
            ),
        ]
    );

    let not_visible = occurrence(C, "helper();", "helper");
    assert!(helper_result.references.iter().all(|reference| {
        reference.source_path != "lib/c.dart" || reference.span.byte_start != not_visible
    }));
}

fn project() -> dartscope_core::DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/a.dart", A),
            DartFileInput::new("lib/b.dart", B),
            DartFileInput::new("lib/c.dart", C),
            DartFileInput::new("lib/external.dart", EXTERNAL),
            DartFileInput::new("lib/base.dart", BASE),
            DartFileInput::new("lib/io.dart", IO),
            DartFileInput::new("lib/conditional.dart", CONDITIONAL),
        ],
        vec![],
    ))
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
        .unwrap_or_else(|| panic!("missing definition result for {path}@{byte_offset}"))
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    let start = source.find(fragment).expect("fragment");
    start
        + source[start..start + fragment.len()]
            .find(token)
            .expect("token")
}

fn nth_occurrence(source: &str, token: &str, index: usize) -> usize {
    source
        .match_indices(token)
        .nth(index)
        .map(|(offset, _)| offset)
        .expect("nth occurrence")
}
