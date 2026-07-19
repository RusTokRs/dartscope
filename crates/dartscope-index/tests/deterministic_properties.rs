use std::collections::BTreeMap;

use dartscope_core::{
    DartFileInput, DartIdentifierReferenceResolutionAnalysis, DartProjectInput,
    DartProjectReferenceAnalysis, DartSymbolQuery, DartSymbolResolutionBasis,
    DartSymbolResolutionStatus,
};
use dartscope_index::{DartWorkspaceIndex, resolve_symbol};
use dartscope_parse::{
    analyze_file_with_references, analyze_project, analyze_project_with_references,
};

const NAMES: [&str; 2] = ["Alpha", "Beta"];
const POLICIES: [Policy; 5] = [
    Policy::All,
    Policy::ShowAlpha,
    Policy::ShowBeta,
    Policy::HideAlpha,
    Policy::HideBeta,
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Policy {
    All,
    ShowAlpha,
    ShowBeta,
    HideAlpha,
    HideBeta,
}

impl Policy {
    fn suffix(self) -> &'static str {
        match self {
            Self::All => "",
            Self::ShowAlpha => " show Alpha",
            Self::ShowBeta => " show Beta",
            Self::HideAlpha => " hide Alpha",
            Self::HideBeta => " hide Beta",
        }
    }

    fn allows(self, name: &str) -> bool {
        match self {
            Self::All => true,
            Self::ShowAlpha => name == "Alpha",
            Self::ShowBeta => name == "Beta",
            Self::HideAlpha => name != "Alpha",
            Self::HideBeta => name != "Beta",
        }
    }

    fn import(self, uri: &str, prefix: Option<&str>) -> String {
        let prefix = prefix.map_or(String::new(), |value| format!(" as {value}"));
        format!("import '{uri}'{prefix}{};\n", self.suffix())
    }

    fn export(self, uri: &str) -> String {
        format!("export '{uri}'{};\n", self.suffix())
    }
}

#[test]
fn generated_combinator_mutations_match_clean_rebuilds() {
    let mut sources = BTreeMap::from([
        (
            "lib/source.dart".to_string(),
            "class Alpha {}\nclass Beta {}\nclass _Private {}\n".to_string(),
        ),
        (
            "lib/barrel.dart".to_string(),
            Policy::All.export("source.dart"),
        ),
        ("lib/client.dart".to_string(), client_source(Policy::All)),
    ]);
    let mut index = DartWorkspaceIndex::from_reference_project(reference_project(&sources));
    assert_snapshot_matches_clean_rebuild(&index, &sources);

    for export_policy in POLICIES {
        let barrel = export_policy.export("source.dart");
        sources.insert("lib/barrel.dart".to_string(), barrel.clone());
        index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
            "lib/barrel.dart",
            barrel,
        )));
        assert_snapshot_matches_clean_rebuild(&index, &sources);

        for import_policy in POLICIES {
            let client = client_source(import_policy);
            sources.insert("lib/client.dart".to_string(), client.clone());
            index.upsert_file_with_references(analyze_file_with_references(DartFileInput::new(
                "lib/client.dart",
                client,
            )));

            assert_snapshot_matches_clean_rebuild(&index, &sources);
            assert_reference_visibility(
                index.snapshot().identifier_reference_resolutions(),
                export_policy,
                import_policy,
            );
        }
    }
}

#[test]
fn generated_direct_prefixed_and_reexport_visibility_is_deterministic() {
    for policy in POLICIES {
        let direct = analyze_project(DartProjectInput::new(
            ".",
            vec![
                declarations(),
                DartFileInput::new("lib/client.dart", policy.import("source.dart", None)),
            ],
            vec![],
        ));
        for name in NAMES {
            assert_resolution(
                &direct,
                DartSymbolQuery::new("lib/client.dart", name),
                policy.allows(name),
                DartSymbolResolutionBasis::DirectImport,
            );
        }
        assert_resolution(
            &direct,
            DartSymbolQuery::new("lib/client.dart", "_Private"),
            false,
            DartSymbolResolutionBasis::DirectImport,
        );

        let prefixed = analyze_project(DartProjectInput::new(
            ".",
            vec![
                declarations(),
                DartFileInput::new("lib/client.dart", policy.import("source.dart", Some("api"))),
            ],
            vec![],
        ));
        for name in NAMES {
            assert_resolution(
                &prefixed,
                DartSymbolQuery::new("lib/client.dart", name).with_prefix("api"),
                policy.allows(name),
                DartSymbolResolutionBasis::DirectImport,
            );
            assert_resolution(
                &prefixed,
                DartSymbolQuery::new("lib/client.dart", name),
                false,
                DartSymbolResolutionBasis::DirectImport,
            );
        }
        assert_resolution(
            &prefixed,
            DartSymbolQuery::new("lib/client.dart", "_Private").with_prefix("api"),
            false,
            DartSymbolResolutionBasis::DirectImport,
        );
    }

    for export_policy in POLICIES {
        for import_policy in POLICIES {
            let project = analyze_project(DartProjectInput::new(
                ".",
                vec![
                    declarations(),
                    DartFileInput::new("lib/barrel.dart", export_policy.export("source.dart")),
                    DartFileInput::new(
                        "lib/client.dart",
                        import_policy.import("barrel.dart", None),
                    ),
                ],
                vec![],
            ));
            for name in NAMES {
                assert_resolution(
                    &project,
                    DartSymbolQuery::new("lib/client.dart", name),
                    export_policy.allows(name) && import_policy.allows(name),
                    DartSymbolResolutionBasis::ReExport,
                );
            }
        }
    }
}

fn declarations() -> DartFileInput {
    DartFileInput::new(
        "lib/source.dart",
        "class Alpha {}\nclass Beta {}\nclass _Private {}\n",
    )
}

fn client_source(policy: Policy) -> String {
    format!(
        "{}void use() {{ Alpha(); Beta(); }}\n",
        policy.import("barrel.dart", None)
    )
}

fn reference_project(sources: &BTreeMap<String, String>) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        sources
            .iter()
            .map(|(path, source)| DartFileInput::new(path.clone(), source.clone()))
            .collect(),
        vec![],
    ))
}

fn assert_snapshot_matches_clean_rebuild(
    index: &DartWorkspaceIndex,
    sources: &BTreeMap<String, String>,
) {
    let baseline = reference_project(sources);
    let fresh = DartWorkspaceIndex::from_reference_project(baseline);
    let incremental = index.snapshot();
    let rebuilt = fresh.snapshot();

    assert_eq!(incremental.project(), rebuilt.project());
    assert_eq!(incremental.uri_graph(), rebuilt.uri_graph());
    assert_eq!(incremental.part_links(), rebuilt.part_links());
    assert_eq!(
        incremental.library_dependency_fingerprints(),
        rebuilt.library_dependency_fingerprints()
    );
    assert_eq!(incremental.graphql_contracts(), rebuilt.graphql_contracts());
    assert_eq!(
        incremental.identifier_reference_resolutions(),
        rebuilt.identifier_reference_resolutions()
    );
}

fn assert_reference_visibility(
    analysis: &DartIdentifierReferenceResolutionAnalysis,
    export_policy: Policy,
    import_policy: Policy,
) {
    for name in NAMES {
        let resolution = analysis
            .resolutions
            .iter()
            .find(|resolution| {
                resolution.reference.source_path == "lib/client.dart"
                    && resolution.reference.name == name
            })
            .unwrap_or_else(|| panic!("missing generated reference for {name}"));
        let visible = export_policy.allows(name) && import_policy.allows(name);
        assert_eq!(
            resolution.status,
            if visible {
                DartSymbolResolutionStatus::Resolved
            } else {
                DartSymbolResolutionStatus::NotVisible
            },
            "{name}, export={export_policy:?}, import={import_policy:?}"
        );
        if visible {
            assert_eq!(resolution.candidates.len(), 1);
            assert_eq!(
                resolution.candidates[0].basis,
                DartSymbolResolutionBasis::ReExport
            );
            assert_eq!(resolution.candidates[0].declaration_path, "lib/source.dart");
        }
    }
}

fn assert_resolution(
    project: &dartscope_core::DartProjectAnalysis,
    query: DartSymbolQuery,
    visible: bool,
    expected_basis: DartSymbolResolutionBasis,
) {
    let resolution = resolve_symbol(project, query.clone());
    let repeated = resolve_symbol(project, query);
    assert_eq!(resolution, repeated);
    assert_eq!(
        resolution.status,
        if visible {
            DartSymbolResolutionStatus::Resolved
        } else {
            DartSymbolResolutionStatus::NotVisible
        }
    );
    if visible {
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].basis, expected_basis);
        assert_eq!(resolution.candidates[0].declaration_path, "lib/source.dart");
    }
}
