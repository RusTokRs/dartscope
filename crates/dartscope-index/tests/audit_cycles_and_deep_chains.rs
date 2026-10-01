// Regression spec from the 2026-09-30 audit: termination and stack safety of library resolution over cycles and very long chains.
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::{DartDefinitionQuery, DartWorkspaceIndex, DartWorkspaceResolutionContext};
use dartscope_parse::analyze_project_with_references;

fn with_timeout<T: Send + 'static>(
    label: &str,
    seconds: u64,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    // A dedicated thread with a modest stack makes a stack overflow visible as a crash of this test binary.
    thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let _ = sender.send(work());
        })
        .expect("spawn");
    match receiver.recv_timeout(Duration::from_secs(seconds)) {
        Ok(value) => Some(value),
        Err(_) => {
            println!("spec {label}: did not finish within {seconds}s");
            None
        }
    }
}

fn offsets(source: &str, needle: &str) -> Vec<usize> {
    source.match_indices(needle).map(|(i, _)| i).collect()
}

#[test]
fn export_cycle_terminates() {
    let c = "import 'a.dart';\nvoid f() { A(); B(); Missing(); }\n";
    let done = with_timeout("export cycle", 30, move || {
        let started = Instant::now();
        let analysis = analyze_project_with_references(DartProjectInput::new(
            ".",
            vec![
                DartFileInput::new("lib/a.dart", "export 'b.dart';\nclass A {}\n"),
                DartFileInput::new("lib/b.dart", "export 'a.dart';\nclass B {}\n"),
                DartFileInput::new("lib/c.dart", c),
            ],
            vec![],
        ));
        let context = DartWorkspaceResolutionContext::new(&analysis);
        let queries: Vec<_> = ["A()", "B()", "Missing()"]
            .iter()
            .flat_map(|n| offsets(c, n))
            .map(|o| DartDefinitionQuery::new("lib/c.dart", o))
            .collect();
        let batch = context.find_definitions(&queries);
        (
            started.elapsed(),
            batch
                .resolutions
                .iter()
                .map(|r| format!("{:?}", r.status))
                .collect::<Vec<_>>(),
        )
    });
    println!("spec C1 export cycle -> {done:?}");
    assert!(done.is_some(), "export cycle did not terminate");
}

#[test]
fn long_reexport_chain_is_resolved_without_stack_overflow() {
    const N: usize = 1500;
    let done = with_timeout("re-export chain", 60, move || {
        let started = Instant::now();
        let mut files = Vec::new();
        for i in 0..N {
            let source = if i + 1 < N {
                format!("export 'f{}.dart';\n", i + 1)
            } else {
                "class Deep {}\n".to_string()
            };
            files.push(DartFileInput::new(format!("lib/f{i}.dart"), source));
        }
        let user = "import 'f0.dart';\nvoid use() { Deep(); }\n";
        files.push(DartFileInput::new("lib/user.dart", user));
        let analysis = analyze_project_with_references(DartProjectInput::new(".", files, vec![]));
        let context = DartWorkspaceResolutionContext::new(&analysis);
        let query = DartDefinitionQuery::new("lib/user.dart", user.find("Deep").unwrap());
        let batch = context.find_definitions(&[query]);
        (
            started.elapsed(),
            format!("{:?}", batch.resolutions[0].status),
        )
    });
    println!("spec C2 {N}-deep export chain -> {done:?}");
    assert!(done.is_some(), "deep export chain crashed or hung");
}

#[test]
fn long_extends_chain_is_handled() {
    const N: usize = 3000;
    let done = with_timeout("extends chain", 60, move || {
        let started = Instant::now();
        let mut source = String::new();
        for i in 0..N {
            source.push_str(&format!("class C{i} extends C{} {{}}\n", i + 1));
        }
        source.push_str(&format!("class C{N} {{ void base() {{}} }}\nclass User extends C0 {{ void go() {{ this.base(); }} }}\n"));
        let analysis = analyze_project_with_references(DartProjectInput::new(
            ".",
            vec![DartFileInput::new("lib/a.dart", source.clone())],
            vec![],
        ));
        let context = DartWorkspaceResolutionContext::new(&analysis);
        let at = source.find("this.base").unwrap() + "this.".len();
        let batch = context.find_definitions(&[DartDefinitionQuery::new("lib/a.dart", at)]);
        (
            started.elapsed(),
            format!("{:?}", batch.resolutions[0].status),
        )
    });
    println!("spec C3 {N}-deep extends chain -> {done:?}");
    assert!(done.is_some());
}

#[test]
fn cyclic_extends_terminates() {
    let done = with_timeout("extends cycle", 20, || {
        let source = "class A extends B { void go() { this.missing(); } }\nclass B extends A {}\n"
            .to_string();
        let analysis = analyze_project_with_references(DartProjectInput::new(
            ".",
            vec![DartFileInput::new("lib/a.dart", source.clone())],
            vec![],
        ));
        let context = DartWorkspaceResolutionContext::new(&analysis);
        let at = source.find("this.missing").unwrap() + "this.".len();
        let batch = context.find_definitions(&[DartDefinitionQuery::new("lib/a.dart", at)]);
        format!("{:?}", batch.resolutions[0].status)
    });
    println!("spec C4 cyclic extends -> {done:?}");
    assert!(done.is_some());
}

#[test]
fn index_update_storm_stays_bounded() {
    let done = with_timeout("update storm", 120, || {
        let files: Vec<_> = (0..400)
            .map(|i| {
                DartFileInput::new(
                    format!("lib/f{i}.dart"),
                    format!("import 'f{}.dart';\nclass F{i} {{}}\n", (i + 1) % 400),
                )
            })
            .collect();
        let analysis = analyze_project_with_references(DartProjectInput::new(".", files, vec![]));
        let mut index = DartWorkspaceIndex::from_reference_project(analysis);
        let started = Instant::now();
        for round in 0..30 {
            let source = format!("import 'f1.dart';\nclass F0 {{ int v{round}; }}\n");
            let update =
                index.upsert_file_with_references(dartscope_parse::analyze_file_with_references(
                    DartFileInput::new("lib/f0.dart", source),
                ));
            let _ = update.affected_paths.len();
        }
        started.elapsed()
    });
    println!("spec C5 30 edits on a 400-file import ring -> {done:?}");
    assert!(done.is_some());
}
