//! Invocation facts copy source text, and nesting multiplies the copies; the copies have a budget.
//!
//! Every argument carries its expression text and every call of a chain carries the dotted prefix
//! before it, so `a(a(a(...)))` and `a().b().c()...` produce facts whose size grows with the square
//! of the nesting or of the chain. Unbounded, a file of 64 KiB produces hundreds of megabytes. Each
//! file may copy 32 times its own size plus 1 MiB for targets and the same for arguments; the facts
//! stop at the first invocation that would exceed it, and a warning says where.

use dartscope_core::{DartFileAnalysis, DartFileInput, DiagnosticSeverity};
use dartscope_parse::analyze_file;

const CODE: &str = "invocation_facts_truncated";

fn analyze(source: String) -> DartFileAnalysis {
    analyze_file(DartFileInput::new("lib/a.dart", source))
}

fn truncation_warnings(analysis: &DartFileAnalysis) -> Vec<&dartscope_core::DartDiagnostic> {
    analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == CODE)
        .collect()
}

fn argument_text(analysis: &DartFileAnalysis) -> usize {
    analysis
        .invocations
        .iter()
        .flat_map(|invocation| &invocation.arguments)
        .map(|argument| argument.expression.len())
        .sum()
}

#[test]
fn calls_nested_thousands_of_levels_deep_are_cut_instead_of_copied_quadratically() {
    // 30,000 levels: the arguments of the outer calls alone would hold about 1.3 GB.
    let depth = 30_000;
    let source = format!("void f() {{ {}{}; }}\n", "a(".repeat(depth), ")".repeat(depth));
    let analysis = analyze(source.clone());

    assert!(analysis.invocations.len() < depth, "{}", analysis.invocations.len());
    assert!(!analysis.invocations.is_empty());
    // The outermost call comes first and carries the whole nest.
    let outermost = &analysis.invocations[0];
    assert_eq!(outermost.target, "a");
    assert!(outermost.arguments[0].expression.starts_with("a("));

    let warnings = truncation_warnings(&analysis);
    assert_eq!(warnings.len(), 1, "{:?}", analysis.diagnostics);
    assert_eq!(warnings[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(warnings[0].path.as_deref(), Some("lib/a.dart"));
    assert!(warnings[0].span.is_some());

    // The last invocation that was kept fits the budget; the facts cannot exceed it by more than one
    // argument list, which is at most the size of the file.
    assert!(
        argument_text(&analysis) <= 33 * source.len() + (1 << 20),
        "{} bytes of argument text",
        argument_text(&analysis)
    );
}

#[test]
fn a_chain_of_thousands_of_calls_is_cut_instead_of_copied_quadratically() {
    // 30,000 calls: the targets alone would hold about 900 MB.
    let source = format!("void f() {{ a{}(); }}\n", "().b".repeat(30_000));
    let analysis = analyze(source.clone());

    assert!(!analysis.invocations.is_empty());
    assert!(analysis.invocations.len() < 30_000, "{}", analysis.invocations.len());
    assert_eq!(analysis.invocations[0].target, "a");
    assert_eq!(analysis.invocations[1].target, "a.b");
    let targets: usize = analysis
        .invocations
        .iter()
        .map(|invocation| invocation.target.len())
        .sum();
    assert!(targets <= 32 * source.len() + (1 << 20), "{targets} bytes of targets");
    assert_eq!(truncation_warnings(&analysis).len(), 1);
}

#[test]
fn nesting_like_a_deep_widget_tree_keeps_every_invocation_and_warns_about_nothing() {
    // 60 screens, each a widget tree 40 levels deep with a sibling at every level: about 19 times
    // the size of the file is copied into the arguments, well under the budget of 32 times.
    let levels = 40;
    let mut source = String::from("import 'package:flutter/material.dart';\n\n");
    let mut expected_calls = 0;
    for screen in 0..60 {
        source.push_str(&format!(
            "Widget screen{screen}() {{\n  return {}Text('leaf'){};\n}}\n\n",
            "Column(children: [Text('sibling'), ".repeat(levels),
            "])".repeat(levels)
        ));
        expected_calls += 2 * levels + 1;
    }
    let analysis = analyze(source);

    assert!(truncation_warnings(&analysis).is_empty(), "{:?}", analysis.diagnostics);
    assert_eq!(analysis.invocations.len(), expected_calls);
    let screen_calls = analysis
        .invocations
        .iter()
        .filter(|invocation| invocation.target == "Column")
        .count();
    assert_eq!(screen_calls, 60 * levels);
}

#[test]
fn the_budget_is_per_file_and_leaves_ordinary_files_alone() {
    let mut source = String::new();
    for index in 0..2000 {
        source.push_str(&format!("void f{index}() {{ g(h({index}), i(j(k(1)))); }}\n"));
    }
    let analysis = analyze(source);
    assert!(truncation_warnings(&analysis).is_empty());
    assert_eq!(analysis.invocations.len(), 2000 * 5);
}
