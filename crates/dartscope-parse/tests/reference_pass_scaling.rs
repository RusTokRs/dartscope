//! Informational scaling measurement of `analyze_file_with_references` (ignored by default).
//!
//! The reference passes answer their per-token questions from lookup structures built once per file,
//! so doubling a file should double the time. This test prints the time for several realistic shapes
//! at four sizes together with the growth per doubling, to compare before and after a change to the
//! passes. It asserts nothing about time; wall-clock thresholds are flaky on shared runners, and the
//! guards against reintroducing a scan per token are the equivalence tests of the lookup structures
//! and the differential check described in `docs/development/fuzzing.md`.
//!
//! ```bash
//! cargo test --release -p dartscope-parse --test reference_pass_scaling -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file_with_references;

/// `n` classes that extend each other and call each other's members.
fn classes(n: usize) -> String {
    let mut source = String::new();
    for class in 0..n {
        let base = if class > 0 {
            format!("C{}", class - 1)
        } else {
            "Object".to_string()
        };
        source.push_str(&format!(
            "class C{class} extends {base} {{\n  int a{class} = 0;\n  int m{class}(int x) {{\n    this.a{class} = x;\n    return this.m0(x) + this.a{class};\n  }}\n  void run{class}() {{\n    this.run0();\n    this.m{class}(1);\n    this.missing();\n    helper();\n  }}\n}}\n"
        ));
    }
    source.push_str("int helper() => 1;\n");
    source
}

/// `n` stateful Flutter widgets with locals, loops, closures, interpolation and member access.
fn widgets(n: usize) -> String {
    let mut source = String::from("import 'package:flutter/material.dart';\n\n");
    for i in 0..n {
        source.push_str(&format!(
            "class W{i} extends StatefulWidget {{\n  const W{i}({{super.key, required this.title}});\n  final String title;\n\n  @override\n  State<W{i}> createState() => _W{i}State();\n}}\n\nclass _W{i}State extends State<W{i}> {{\n  int _count = 0;\n  final List<String> _items = [];\n\n  void _increment(int step) {{\n    var total = _count + step;\n    for (var i = 0; i < step; i++) {{\n      total += i;\n    }}\n    setState(() {{\n      _count = total;\n      _items.add('item $total');\n    }});\n  }}\n\n  @override\n  Widget build(BuildContext context) {{\n    final label = '${{widget.title}}: $_count';\n    return Column(children: [\n      Text(label),\n      ..._items.map((item) => Text(item)),\n      ElevatedButton(onPressed: () => _increment(1), child: const Text('+')),\n    ]);\n  }}\n}}\n\n"
        ));
    }
    source
}

/// One method with `n` groups of statements that declare, update and read locals.
fn statements(n: usize) -> String {
    let mut source = String::from("class S {\n  int field = 0;\n  void run(int seed) {\n");
    for i in 0..n {
        source.push_str(&format!(
            "    var v{i} = seed + field;\n    v{i} += {i};\n    field = v{i};\n    print(v{i});\n"
        ));
    }
    source.push_str("  }\n}\n");
    source
}

/// One `build` method that returns a single nested expression with `n` children.
fn tree(n: usize) -> String {
    let mut source = String::from(
        "class T extends StatelessWidget {\n  final String title = 'x';\n  Widget build(BuildContext context) {\n    return Column(children: [\n",
    );
    for i in 0..n {
        source.push_str(&format!(
            "      Padding(padding: EdgeInsets.all({i}), child: Text(title + '{i}')),\n"
        ));
    }
    source.push_str("    ]);\n  }\n}\n");
    source
}

/// One expression with `n` terms and no delimiter in front of the end of the statement.
fn long_expression(n: usize) -> String {
    let mut source = String::from("int run(int a) {\n  return a");
    for i in 0..n {
        source.push_str(if i % 7 == 0 { " + f(a)" } else { " + a" });
    }
    source.push_str(";\n}\nint f(int a) => a;\n");
    source
}

/// One method with `n` groups of closures, `for` loops and a `try`.
fn closures(n: usize) -> String {
    let mut source = String::from("class C {\n  void run(List<int> items) {\n");
    for i in 0..n {
        source.push_str(&format!(
            "    items.map((x) => x + {i}).toList();\n    items.forEach((y) {{ print(y); }});\n    for (var i = 0; i < {i}; i++) {{ print(i); }}\n    for (final z in items) {{ print(z); }}\n    try {{ run(items); }} catch (e, s) {{ print(e); }}\n"
        ));
    }
    source.push_str("  }\n}\n");
    source
}

/// `n` top-level functions with arrow bodies that call their predecessor.
fn functions(n: usize) -> String {
    let mut source = String::from("int f0(int a) => a;\n");
    for i in 1..n {
        source.push_str(&format!("int f{i}(int a) => f{}(a) + a;\n", i - 1));
    }
    source
}

/// A named generator of source text and the size to start from.
type Shape = (&'static str, fn(usize) -> String, usize);

fn time(source: &str) -> Duration {
    let started = Instant::now();
    let analysis =
        analyze_file_with_references(DartFileInput::new("lib/a.dart", source.to_string()));
    let elapsed = started.elapsed();
    assert!(!analysis.references.is_empty());
    elapsed
}

#[test]
#[ignore = "informational timing; run with --ignored --nocapture"]
fn print_growth_of_the_reference_analysis_per_doubling() {
    let shapes: [Shape; 6] = [
        ("classes", classes, 500),
        ("widgets", widgets, 100),
        ("statements", statements, 1000),
        ("tree", tree, 1000),
        ("long_expression", long_expression, 2000),
        ("closures", closures, 500),
    ];
    for (name, generate, base) in shapes {
        let mut previous: Option<Duration> = None;
        for step in 0..4 {
            let n = base << step;
            let source = generate(n);
            let elapsed = time(&source);
            let growth = previous.map_or_else(
                || "-".to_string(),
                |before| {
                    format!(
                        "x{:.1}",
                        elapsed.as_secs_f64() / before.as_secs_f64().max(1e-9)
                    )
                },
            );
            println!(
                "{name:>16} n={n:<6} bytes={:<8} time={elapsed:>10.2?} growth_per_doubling={growth}",
                source.len()
            );
            previous = Some(elapsed);
        }
    }
    // The function shape is covered separately because its declarations are all top-level.
    let source = functions(8000);
    println!("{:>16} n=8000   time={:.2?}", "functions", time(&source));
}
