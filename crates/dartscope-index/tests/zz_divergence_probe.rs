//! TEMPORARY probe (removed before hand-off): which resolutions differ after a part directive is removed or added.
use dartscope_core::{
    DartFileInput, DartIdentifierReferenceResolutionAnalysis, DartProjectInput,
    DartProjectReferenceAnalysis,
};
use dartscope_index::{
    DartIndexOptions, DartWorkspaceIndex, resolve_project_identifier_references_with_options,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

const A0: &str = "library lib_a;\n\npart 'b.dart';\n\nclass Owner {\n  void use() {\n    inPart();\n    FromPart().touch();\n  }\n}\n";
const A_NO_PART: &str = "library lib_a;\n\nclass Owner {\n  void use() {\n    inPart();\n    FromPart().touch();\n  }\n}\n";
const B0: &str = "part of 'a.dart';\n\nvoid inPart() {}\n\nclass FromPart {\n  void touch() {}\n}\n";
const B_CUT: &str = "part of 'a.dart';\n\nvoid inPart() {}\n\nclass FromPart {";

fn project(a: &str, b: &str) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/a.dart", a),
            DartFileInput::new("lib/b.dart", b),
        ],
        vec![],
    ))
}

fn lines(analysis: &DartIdentifierReferenceResolutionAnalysis) -> Vec<String> {
    analysis
        .resolutions
        .iter()
        .map(|r| {
            format!(
                "{}:{} {} {:?} [{}]",
                r.reference.source_path.trim_start_matches("lib/"),
                r.reference.span.byte_start,
                r.reference.name,
                r.status,
                r.candidates
                    .iter()
                    .map(|c| format!("{}:{}", c.declaration_path.trim_start_matches("lib/"), c.name))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect()
}

fn probe(label: &str, from: (&str, &str), to: (&str, &str), a_first: bool, out: &mut Vec<String>) {
    let mut index = DartWorkspaceIndex::from_reference_project(project(from.0, from.1));
    let upsert_a = |index: &mut DartWorkspaceIndex| {
        if to.0 != from.0 {
            let _ = index.upsert_file_with_references(analyze_file_with_references(
                DartFileInput::new("lib/a.dart", to.0),
            ));
        }
    };
    let upsert_b = |index: &mut DartWorkspaceIndex| {
        if to.1 != from.1 {
            let _ = index.upsert_file_with_references(analyze_file_with_references(
                DartFileInput::new("lib/b.dart", to.1),
            ));
        }
    };
    if a_first {
        upsert_a(&mut index);
        upsert_b(&mut index);
    } else {
        upsert_b(&mut index);
        upsert_a(&mut index);
    }
    let fresh = project(to.0, to.1);
    let expected = lines(&resolve_project_identifier_references_with_options(
        &fresh,
        &DartIndexOptions::default(),
    ));
    let actual = lines(index.snapshot().identifier_reference_resolutions());
    let only_fresh: Vec<_> = expected.iter().filter(|l| !actual.contains(l)).collect();
    let only_incremental: Vec<_> = actual.iter().filter(|l| !expected.contains(l)).collect();
    if !only_fresh.is_empty() || !only_incremental.is_empty() {
        out.push(format!("[{label}]"));
        for line in only_fresh {
            out.push(format!("  fresh only: {line}"));
        }
        for line in only_incremental {
            out.push(format!("  incremental only: {line}"));
        }
    }
}

#[test]
fn probe_part_divergence() {
    let mut out = Vec::new();
    probe("remove part directive", (A0, B0), (A_NO_PART, B0), true, &mut out);
    probe("add part directive", (A_NO_PART, B0), (A0, B0), true, &mut out);
    probe("a1=' FromPart()', b unchanged", (A0, B0), (" FromPart()", B0), true, &mut out);
    probe("a1=' FromPart()', b cut, a first", (A0, B0), (" FromPart()", B_CUT), true, &mut out);
    probe("a1=' FromPart()', b cut, b first", (A0, B0), (" FromPart()", B_CUT), false, &mut out);
    assert!(out.is_empty(), "\n{}", out.join("\n"));
}
