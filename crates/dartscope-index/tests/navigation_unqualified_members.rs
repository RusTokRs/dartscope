use dartscope_core::{
    DartDeclarationKind, DartFileInput, DartIdentifierReferenceKind, DartProjectInput,
    DartProjectReferenceAnalysis, DartSymbolResolutionBasis,
};
use dartscope_index::{
    DartDefinitionQuery, DartDefinitionResolution, DartDefinitionResolutionStatus,
    DartDefinitionTarget, DartWorkspaceIndex, DartWorkspaceResolutionContext,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

const SERVICE: &str = r#"
class Service {
  int count = 0;
  static int total = 0;

  void work() {}

  static void build() {}

  void exercise() {
    work();
    count = count + 1;
    build();
    total = 2;
    absent();
  }
}
"#;

const RENAMED_SERVICE: &str = r#"
class Service {
  int count = 0;
  static int total = 0;

  void labour() {}

  static void build() {}

  void exercise() {
    work();
    count = count + 1;
    build();
    total = 2;
    absent();
  }
}
"#;

#[test]
fn resolves_unqualified_same_owner_member_targets() {
    let analysis = project(SERVICE);
    let context = DartWorkspaceResolutionContext::new(&analysis);
    let work = occurrence(SERVICE, "work();\n    count", "work");
    let write = nth_occurrence(SERVICE, "count = count + 1;", "count", 0);
    let read = nth_occurrence(SERVICE, "count = count + 1;", "count", 1);
    let build = occurrence(SERVICE, "build();", "build");
    let total = occurrence(SERVICE, "total = 2;", "total");
    let absent = occurrence(SERVICE, "absent();", "absent");
    let batch = context.find_definitions(&[
        DartDefinitionQuery::new("lib/service.dart", work),
        DartDefinitionQuery::new("lib/service.dart", read),
        DartDefinitionQuery::new("lib/service.dart", write),
        DartDefinitionQuery::new("lib/service.dart", build),
        DartDefinitionQuery::new("lib/service.dart", total),
        DartDefinitionQuery::new("lib/service.dart", absent),
    ]);

    assert_member_target(
        resolution_at(&batch.resolutions, work),
        DartDefinitionResolutionStatus::Resolved,
        "work",
        DartDeclarationKind::Method,
    );
    assert_member_target(
        resolution_at(&batch.resolutions, read),
        DartDefinitionResolutionStatus::Resolved,
        "count",
        DartDeclarationKind::Field,
    );
    assert_member_target(
        resolution_at(&batch.resolutions, write),
        DartDefinitionResolutionStatus::Resolved,
        "count",
        DartDeclarationKind::Field,
    );
    assert_member_target(
        resolution_at(&batch.resolutions, build),
        DartDefinitionResolutionStatus::Resolved,
        "build",
        DartDeclarationKind::Method,
    );
    assert_member_target(
        resolution_at(&batch.resolutions, total),
        DartDefinitionResolutionStatus::Resolved,
        "total",
        DartDeclarationKind::Field,
    );
    assert_eq!(
        resolution_at(&batch.resolutions, absent).status,
        DartDefinitionResolutionStatus::Missing
    );

    let work_target = resolution_at(&batch.resolutions, work).targets[0].clone();
    let references = context.find_references(std::slice::from_ref(&work_target));
    assert_eq!(references.results.len(), 1);
    assert!(references.results[0].references.iter().any(|reference| {
        reference.span.byte_start == work
            && reference.kind == DartIdentifierReferenceKind::MemberInvocationInstance
    }));
}

#[test]
fn incremental_snapshots_match_full_unqualified_member_resolution() {
    let initial = project(SERVICE);
    let mut index = DartWorkspaceIndex::from_reference_project(initial.clone());
    let snapshot = index.snapshot();
    assert_eq!(snapshot.identifier_references(), initial.references);

    let work = occurrence(SERVICE, "work();\n    count", "work");
    let renamed_work = occurrence(RENAMED_SERVICE, "work();\n    count", "work");
    let initial_queries = [DartDefinitionQuery::new("lib/service.dart", work)];
    let renamed_queries = [DartDefinitionQuery::new("lib/service.dart", renamed_work)];

    assert_eq!(
        DartWorkspaceResolutionContext::from_snapshot(&snapshot).find_definitions(&initial_queries),
        DartWorkspaceResolutionContext::new(&initial).find_definitions(&initial_queries)
    );

    let update = index.upsert_file_with_references(analyze_file_with_references(
        DartFileInput::new("lib/service.dart", RENAMED_SERVICE),
    ));
    assert!(update.rebuilt.identifier_references);
    let renamed_full = project(RENAMED_SERVICE);
    let renamed_snapshot = index.snapshot();
    let incremental = DartWorkspaceResolutionContext::from_snapshot(&renamed_snapshot)
        .find_definitions(&renamed_queries);
    assert_eq!(
        incremental,
        DartWorkspaceResolutionContext::new(&renamed_full).find_definitions(&renamed_queries)
    );
    assert_eq!(
        incremental.resolutions[0].status,
        DartDefinitionResolutionStatus::Missing
    );
}

fn project(source: &str) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![DartFileInput::new("lib/service.dart", source)],
        vec![],
    ))
}

fn assert_member_target(
    resolution: &DartDefinitionResolution,
    status: DartDefinitionResolutionStatus,
    name: &str,
    kind: DartDeclarationKind,
) {
    assert_eq!(resolution.status, status, "{name:?}");
    assert_eq!(resolution.targets.len(), 1, "{name:?}");
    let DartDefinitionTarget::Namespace(candidate) = &resolution.targets[0] else {
        panic!("expected a namespace target for {name:?}");
    };
    assert_eq!(candidate.name, name);
    assert_eq!(candidate.kind, kind);
    assert_eq!(candidate.declaration_path, "lib/service.dart");
    assert_eq!(candidate.basis, DartSymbolResolutionBasis::SameFile);
}

fn resolution_at(
    resolutions: &[DartDefinitionResolution],
    byte_offset: usize,
) -> &DartDefinitionResolution {
    resolutions
        .iter()
        .find(|resolution| resolution.query.byte_offset == byte_offset)
        .unwrap_or_else(|| panic!("missing resolution at byte {byte_offset}"))
}

fn occurrence(source: &str, fragment: &str, token: &str) -> usize {
    nth_occurrence(source, fragment, token, 0)
}

fn nth_occurrence(source: &str, fragment: &str, token: &str, index: usize) -> usize {
    let mut search_start = source.find(fragment).expect("fragment");
    for position in 0..=index {
        let found = source[search_start..]
            .find(token)
            .expect("token occurrence")
            + search_start;
        if position == index {
            return found;
        }
        search_start = found + token.len();
    }
    unreachable!("token occurrence index is bounded by the loop")
}
