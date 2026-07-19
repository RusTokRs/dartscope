use std::env;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

use dartscope::{
    DartFileInput, DartProjectInput, DartWorkspaceIndex, analyze_file, analyze_project,
    analyze_project_with_references, resolve_project_identifier_references,
};

const PROJECT_FILE_COUNT: usize = 600;

fn main() -> ExitCode {
    let Some(workload) = env::args().nth(1) else {
        eprintln!("usage: quality_benchmark <parse|index|references>");
        return ExitCode::from(2);
    };

    let observation = match workload.as_str() {
        "parse" => benchmark_parse(),
        "index" => benchmark_index(),
        "references" => benchmark_references(),
        _ => {
            eprintln!("unknown workload: {workload}");
            return ExitCode::from(2);
        }
    };

    println!(
        "{{\"workload\":\"{}\",\"elapsed_ns\":{},\"units\":{},\"digest\":{}}}",
        observation.workload, observation.elapsed_ns, observation.units, observation.digest
    );
    ExitCode::SUCCESS
}

struct Observation {
    workload: &'static str,
    elapsed_ns: u128,
    units: usize,
    digest: usize,
}

fn benchmark_parse() -> Observation {
    let source = synthetic_parse_source(320);
    let iterations = 24;
    let started = Instant::now();
    let mut digest = 0usize;

    for iteration in 0..iterations {
        let analysis = black_box(analyze_file(DartFileInput::new(
            format!("lib/parse_{iteration:02}.dart"),
            source.clone(),
        )));
        digest = digest.wrapping_add(
            analysis.declarations.len() * 17
                + analysis.invocations.len() * 13
                + analysis.imports.len() * 7
                + analysis.diagnostics.len(),
        );
    }

    Observation {
        workload: "parse",
        elapsed_ns: started.elapsed().as_nanos(),
        units: iterations * source.lines().count(),
        digest,
    }
}

fn benchmark_index() -> Observation {
    let project = analyze_project(synthetic_project_input(PROJECT_FILE_COUNT));
    let iterations = 4;
    let started = Instant::now();
    let mut digest = 0usize;

    for _ in 0..iterations {
        let workspace = black_box(DartWorkspaceIndex::from_project(project.clone()));
        let metrics = workspace.retained_metrics();
        digest = digest.wrapping_add(
            metrics.indexed_files * 31
                + metrics.uri_references * 19
                + metrics.library_entries * 11
                + metrics.dependency_references * 5
                + metrics.reference_resolutions,
        );
    }

    Observation {
        workload: "index",
        elapsed_ns: started.elapsed().as_nanos(),
        units: iterations * PROJECT_FILE_COUNT,
        digest,
    }
}

fn benchmark_references() -> Observation {
    let analysis = analyze_project_with_references(synthetic_project_input(PROJECT_FILE_COUNT));
    assert_eq!(analysis.references.len(), PROJECT_FILE_COUNT - 1);
    let iterations = 8;
    let started = Instant::now();
    let mut digest = 0usize;

    for _ in 0..iterations {
        let resolutions = black_box(resolve_project_identifier_references(&analysis));
        let candidate_count: usize = resolutions
            .resolutions
            .iter()
            .map(|resolution| resolution.candidates.len())
            .sum();
        digest = digest.wrapping_add(resolutions.resolutions.len() * 23 + candidate_count * 3);
    }

    Observation {
        workload: "references",
        elapsed_ns: started.elapsed().as_nanos(),
        units: iterations * analysis.references.len(),
        digest,
    }
}

fn synthetic_parse_source(item_count: usize) -> String {
    let mut source = String::from("import 'dart:async';\n");
    for index in 0..item_count {
        source.push_str(&format!(
            "class Type{index:03} {{\n  int value = {index};\n  Future<int> compute(int input) async {{\n    helper{index:03}(input);\n    return input + value;\n  }}\n}}\nvoid helper{index:03}(int value) {{ print(value); }}\n"
        ));
    }
    source
}

fn synthetic_project_input(file_count: usize) -> DartProjectInput {
    let files = (0..file_count)
        .map(|index| {
            let mut source = String::new();
            if index > 0 {
                source.push_str(&format!("import 'file_{:05}.dart';\n", index - 1));
            }
            source.push_str(&format!("void symbol_{index:05}() {{}}\n"));
            if index > 0 {
                source.push_str(&format!(
                    "void invoke_{index:05}() {{ symbol_{:05}(); }}\n",
                    index - 1
                ));
            }
            DartFileInput::new(format!("lib/file_{index:05}.dart"), source)
        })
        .collect();

    DartProjectInput::new(".", files, Vec::new())
}
