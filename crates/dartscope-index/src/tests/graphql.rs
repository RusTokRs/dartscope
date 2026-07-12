use crate::*;
use dartscope_core::*;
use dartscope_parse::analyze_project;

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
    let private_operation = "const _privateQuery = r'''query PrivateViewer { viewer { id } }''';\n";
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
