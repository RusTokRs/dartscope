//! TEMPORARY development-loop measurement (removed before hand-off): how the reference
//! resolution of a whole project scales with its size.
use std::time::Instant;

use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_index::DartWorkspaceResolutionContext;
use dartscope_parse::analyze_project_with_references;

fn project(files: usize, classes: usize) -> DartProjectInput {
    let mut inputs = Vec::new();
    for file in 0..files {
        let mut source = String::new();
        for offset in 1..=3 {
            source.push_str(&format!("import 'f{}.dart';\n", (file + offset) % files));
        }
        for class in 0..classes {
            let base = if class > 0 {
                format!("F{file}C{}", class - 1)
            } else if file % 4 != 3 {
                format!("F{}C{}", file + 1, classes - 1)
            } else {
                "Object".to_string()
            };
            source.push_str(&format!(
                "class F{file}C{class} extends {base} {{\n  int a{class} = 0;\n  int m{class}(int x) {{\n    this.a{class} = x;\n    return this.m0(x) + this.a{class};\n  }}\n  void run{class}() {{\n    this.run0();\n    this.m{class}(1);\n    this.missing();\n    helper{file}();\n  }}\n}}\n"
            ));
        }
        source.push_str(&format!("int helper{file}() => 1;\n"));
        inputs.push(DartFileInput::new(format!("lib/f{file}.dart"), source));
    }
    DartProjectInput::new(".", inputs, vec![])
}

#[test]
#[ignore = "development-loop measurement"]
fn index_scaling() {
    for (files, classes) in [(25, 40), (50, 40), (100, 40), (200, 40)] {
        let input = project(files, classes);
        let started = Instant::now();
        let analysis = analyze_project_with_references(input);
        let parse = started.elapsed();
        let started = Instant::now();
        let context = DartWorkspaceResolutionContext::new(&analysis);
        let index = started.elapsed();
        let declarations: usize = analysis
            .project
            .files
            .iter()
            .map(|file| file.declarations.len())
            .sum();
        println!(
            "files={files} classes_per_file={classes} declarations={declarations} references={} parse={parse:?} index={index:?}",
            analysis.references.len()
        );
        std::hint::black_box(&context);
    }
}
