use crate::*;
use dartscope_core::*;
use dartscope_parse::analyze_project;

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
    assert!(
        analysis
            .links
            .iter()
            .all(|link| link.status == DartPartLinkStatus::Matched)
    );
    assert!(
        analysis
            .links
            .iter()
            .all(|link| link.part_of_span.is_some())
    );
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
