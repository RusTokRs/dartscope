//! TEMPORARY probe (removed before hand-off): which fields of the resolutions differ after an edit.
use dartscope_core::{
    DartFileInput, DartIdentifierReferenceResolution, DartProjectInput,
    DartProjectReferenceAnalysis,
};
use dartscope_index::{
    DartIndexOptions, DartWorkspaceIndex, resolve_project_identifier_references_with_options,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

const A0: &str = "library lib_a;\n\npart 'b.dart';\n\nclass Owner {\n  void use() {\n    inPart();\n    FromPart().touch();\n  }\n}\n";
const B0: &str = "part of 'a.dart';\n\nvoid inPart() {}\n\nclass FromPart {\n  void touch() {}\n}\n";
const B_LINUX: &str = "part of 'a.dart';\n\nvoid inPart() {}\n\nclass FromPart {";
const B_MAC: &str = "part of 'a.dart';\n\nvoid inPart() {}\n\nclass FromPart )";

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

fn fields(r: &DartIdentifierReferenceResolution) -> Vec<(String, String)> {
    let mut out = vec![
        ("status".to_string(), format!("{:?}", r.status)),
        ("kind".to_string(), format!("{:?}", r.reference.kind)),
        ("confidence".to_string(), format!("{:?}", r.reference.confidence)),
        ("prefix".to_string(), format!("{:?}", r.reference.prefix)),
        (
            "enclosing".to_string(),
            format!("{:?}", r.reference.enclosing_symbol_id),
        ),
        (
            "span".to_string(),
            format!("{:?}", (r.reference.span.byte_start, r.reference.span.byte_end)),
        ),
        ("candidates".to_string(), r.candidates.len().to_string()),
    ];
    for (i, c) in r.candidates.iter().enumerate() {
        out.push((
            format!("cand{i}"),
            format!(
                "{} {:?} {:?} {} {:?} {:?}",
                c.name,
                c.kind,
                c.symbol_id,
                c.declaration_path,
                (c.declaration_span.byte_start, c.declaration_span.byte_end),
                c.basis
            ),
        ));
    }
    out
}

fn probe(label: &str, from: (&str, &str), to: (&str, &str), a_first: bool, out: &mut Vec<String>) {
    let mut index = DartWorkspaceIndex::from_reference_project(project(from.0, from.1));
    let mut upsert = |path: &str, text: &str, old: &str| {
        if text != old {
            let _ = index.upsert_file_with_references(analyze_file_with_references(
                DartFileInput::new(path, text),
            ));
        }
    };
    if a_first {
        upsert("lib/a.dart", to.0, from.0);
        upsert("lib/b.dart", to.1, from.1);
    } else {
        upsert("lib/b.dart", to.1, from.1);
        upsert("lib/a.dart", to.0, from.0);
    }
    let fresh = project(to.0, to.1);
    let expected =
        resolve_project_identifier_references_with_options(&fresh, &DartIndexOptions::default());
    let snapshot = index.snapshot();
    let actual = snapshot.identifier_reference_resolutions();
    if &expected == actual {
        return;
    }
    out.push(format!(
        "[{label}] counts fresh={} incremental={}",
        expected.resolutions.len(),
        actual.resolutions.len()
    ));
    let key = |r: &DartIdentifierReferenceResolution| {
        (
            r.reference.source_path.clone(),
            r.reference.span.byte_start,
            r.reference.name.clone(),
        )
    };
    for fresh_r in &expected.resolutions {
        match actual.resolutions.iter().find(|r| key(r) == key(fresh_r)) {
            None => out.push(format!("  missing in incremental: {:?}", key(fresh_r))),
            Some(inc_r) if inc_r != fresh_r => {
                let (f, i) = (fields(fresh_r), fields(inc_r));
                for n in 0..f.len().max(i.len()) {
                    if f.get(n) != i.get(n) {
                        out.push(format!(
                            "  {:?}: fresh={:?} incremental={:?}",
                            key(fresh_r),
                            f.get(n),
                            i.get(n)
                        ));
                    }
                }
            }
            Some(_) => {}
        }
    }
    for inc_r in &actual.resolutions {
        if !expected.resolutions.iter().any(|r| key(r) == key(inc_r)) {
            out.push(format!("  extra in incremental: {:?}", key(inc_r)));
        }
    }
}

#[test]
fn probe_part_divergence() {
    let mut out = Vec::new();
    probe("a1 only", (A0, B0), (" FromPart()", B0), true, &mut out);
    probe("linux, a first", (A0, B0), (" FromPart()", B_LINUX), true, &mut out);
    probe("linux, b first", (A0, B0), (" FromPart()", B_LINUX), false, &mut out);
    probe("mac, a first", (A0, B0), (" FromPart()", B_MAC), true, &mut out);
    probe("mac, b first", (A0, B0), (" FromPart()", B_MAC), false, &mut out);
    assert!(out.is_empty(), "\n{}", out.join("\n"));
}
