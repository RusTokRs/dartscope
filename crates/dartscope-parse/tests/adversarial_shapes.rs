//! Informational scaling sweep over hostile source shapes (ignored by default).
//!
//! `reference_pass_scaling.rs` measures realistic files. This sweep feeds the analysis the shapes
//! that broken or hostile input produces: thousands of unclosed delimiters, deep nesting, one huge
//! token, long chains. For each shape it prints the time of `analyze_file` and of
//! `analyze_file_with_references` at five sizes together with the growth of the second per doubling,
//! so a quadratic stage shows as a growth near 4.
//!
//! Every measurement runs in a child process (this test binary again, selected through environment
//! variables), so a stack overflow, an out-of-memory kill or a runaway loop is reported for its shape
//! instead of ending the sweep. Nothing about time is asserted; wall-clock thresholds are flaky on
//! shared runners. The tests that guard the fixes are the equivalence tests next to each lookup
//! structure.
//!
//! ```bash
//! cargo test --release -p dartscope-parse --test adversarial_shapes -- --ignored --nocapture
//! ```
//!
//! `ADVERSARIAL_ONLY=a,b` restricts the sweep to the named shapes, `ADVERSARIAL_TIMEOUT_SECS` changes
//! the limit of one measurement (default 10) and `ADVERSARIAL_SIZES` the comma-separated sizes in
//! KiB (default `64,128,256,512,1024`).

use std::env;
use std::hint::black_box;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dartscope_core::DartFileInput;
use dartscope_parse::{analyze_file, analyze_file_with_references};

/// `head`, then `open` repeated `n` times, then `close` repeated `n` times, then `tail`.
struct Shape {
    name: &'static str,
    head: &'static str,
    open: &'static str,
    close: &'static str,
    tail: &'static str,
}

const fn shape(
    name: &'static str,
    head: &'static str,
    open: &'static str,
    close: &'static str,
    tail: &'static str,
) -> Shape {
    Shape {
        name,
        head,
        open,
        close,
        tail,
    }
}

const SHAPES: &[Shape] = &[
    // Delimiters on one line, unclosed and nested.
    shape("unclosed_calls", "void f() { ", "a(", "", " }"),
    shape("nested_calls", "void f() { ", "a(", ")", "; }"),
    shape("unclosed_dotted_calls", "void f() { ", "x.a(", "", " }"),
    shape("unclosed_angles", "void f() { var x = ", "a<", "", "; }"),
    shape("nested_generic_calls", "void f() { ", "a<", ">", "(); }"),
    shape("unclosed_brackets", "void f() { var x = ", "[", "", "; }"),
    shape("nested_lists", "void f() { var x = ", "[", "]", "; }"),
    shape("unclosed_braces", "void f() ", "{ ", "", ""),
    shape("nested_blocks", "void f() ", "{ ", "} ", ""),
    shape("unclosed_maps", "var x = ", "{'a': ", "", ";"),
    shape("nested_maps", "var x = ", "{'a': ", "}", ";"),
    shape("nested_parentheses", "int f() => ", "(", ")", ";"),
    shape("unclosed_strings", "void f() { ", "'a ", "", "}"),
    shape("unclosed_interpolations", "var s = ", "'${", "", ""),
    shape("nested_interpolations", "var s = ", "'${", "}'", ";"),
    shape("unclosed_block_comments", "", "/* ", "", ""),
    shape("nested_block_comments", "", "/* ", "*/ ", ""),
    shape("unclosed_annotations", "", "@A(", "", " class C {}"),
    shape("nested_annotations", "", "@A(", ")", " class C {}"),
    shape("unclosed_generic_params", "class A<", "T extends B<", "", ""),
    shape("unclosed_function_types", "typedef F = ", "void Function(", "", ""),
    shape("nested_records", "var x = ", "(1, ", ")", ";"),
    shape("unclosed_patterns", "void f() { switch (x) { case ", "[", "", " }"),
    // Long chains and expressions without any delimiter to stop at.
    shape("long_member_chain", "void f() { a", ".b", "", "(); }"),
    shape("long_call_chain", "void f() { a", "().b", "", "(); }"),
    shape("long_cascade", "void f() { a", "..b()", "", "; }"),
    shape("long_ternary", "int f() => a", " ? b : c", "", ";"),
    shape("nested_ternary", "int f() => ", "a ? ", "", "b : c;"),
    shape("long_binary_expression", "int f() => a", " + a", "", ";"),
    shape("long_identifier", "class A", "a", "", " {}"),
    shape("long_string_literal", "var s = '", "a", "", "';"),
    shape("long_comment_line", "// ", "a", "", "\nclass A {}"),
    shape("long_generic_arguments", "var x = <", "int, ", "", "int>[];"),
    // Nested statements.
    shape("unclosed_closures", "void f() { ", "g((x) { ", "", ""),
    shape("nested_closures", "void f() { ", "g((x) { ", "}); ", "}"),
    shape(
        "nested_for_statements",
        "void f() { ",
        "for (var i = 0; i < 1; i++) ",
        "",
        "{} }",
    ),
    shape("else_if_chain", "void f() { ", "if (a) {} else ", "", "{} }"),
    shape("nested_if_statements", "void f() { ", "if (a) ", "", "g(); }"),
    // Line-oriented shapes.
    shape(
        "nested_class_headers",
        "",
        "class A<T extends B<T>> extends C with D implements E {\n",
        "}\n",
        "",
    ),
    shape("unclosed_classes", "", "class A {\n  void m() {\n", "", ""),
    shape("unclosed_methods", "class A {\n", "  void m() {\n    g(\n", "", ""),
    shape("unterminated_imports", "", "import 'a.dart'\n", "", ""),
    shape("many_annotation_lines", "", "@A\n", "", "class C {}"),
    shape("many_doc_comment_lines", "", "/// doc\n", "", "class C {}"),
    shape("many_blank_lines", "", "\n", "", "class C {}"),
    shape("many_enum_constants", "enum E {\n", "  a(1),\n", "", "  z;\n}"),
    shape(
        "many_switch_cases",
        "void f() { switch (x) {\n",
        "  case 1: g(); break;\n",
        "",
        "} }",
    ),
    shape(
        "many_try_statements",
        "void f() {\n",
        "  try { g(); } catch (e) {}\n",
        "",
        "}",
    ),
    shape(
        "many_accessors",
        "class A {\n",
        "  int get x => 1;\n  set x(int v) {}\n",
        "",
        "}",
    ),
    shape(
        "many_operators",
        "class A {\n",
        "  bool operator ==(Object o) => true;\n",
        "",
        "}",
    ),
    shape(
        "many_constructors",
        "class A {\n",
        "  A.n() : a = 1, b = 2;\n",
        "",
        "}",
    ),
    shape("many_typedefs", "", "typedef F = void Function(int);\n", "", ""),
    shape(
        "many_extensions",
        "",
        "extension E on A { void m() {} }\n",
        "",
        "",
    ),
    shape("many_mixins", "", "mixin M on A { }\n", "", ""),
    shape(
        "many_static_constants",
        "class A {\n",
        "  static const a = 'x';\n",
        "",
        "}",
    ),
    shape("many_string_constants", "", "const s = 'x';\n", "", ""),
    shape(
        "many_graphql_documents",
        "",
        "final q = gql(r'''query Q { a }''');\n",
        "",
        "",
    ),
];

fn build(shape: &Shape, bytes: usize) -> String {
    let unit = (shape.open.len() + shape.close.len()).max(1);
    let count = bytes.saturating_sub(shape.head.len() + shape.tail.len()) / unit;
    let mut source = String::with_capacity(bytes + 64);
    source.push_str(shape.head);
    for _ in 0..count {
        source.push_str(shape.open);
    }
    for _ in 0..count {
        source.push_str(shape.close);
    }
    source.push_str(shape.tail);
    source
}

/// Run by the sweep in a child process; does nothing when the sweep did not select a shape.
#[test]
#[ignore = "child of the sweep; does nothing without ADVERSARIAL_SHAPE"]
fn child_measures_one_shape() {
    let (Ok(name), Ok(bytes)) = (env::var("ADVERSARIAL_SHAPE"), env::var("ADVERSARIAL_BYTES"))
    else {
        return;
    };
    let shape = SHAPES
        .iter()
        .find(|shape| shape.name == name)
        .expect("a known shape");
    let source = build(shape, bytes.parse().expect("a size"));

    let started = Instant::now();
    let analysis = analyze_file(DartFileInput::new("lib/a.dart", source.clone()));
    let base = started.elapsed();
    let declarations = analysis.declarations.len();
    let invocations = analysis.invocations.len();
    black_box(&analysis);
    drop(analysis);

    let started = Instant::now();
    let full = analyze_file_with_references(DartFileInput::new("lib/a.dart", source));
    let with_references = started.elapsed();
    let references = full.references.len();
    black_box(&full);

    println!(
        "RESULT {} {} {declarations} {invocations} {references}",
        base.as_nanos(),
        with_references.as_nanos()
    );
}

enum Outcome {
    Done {
        base: Duration,
        full: Duration,
        counts: String,
    },
    TimedOut,
    Failed(String),
}

fn measure(name: &str, bytes: usize, timeout: Duration) -> Outcome {
    let executable = env::current_exe().expect("the test binary");
    let mut child = Command::new(executable)
        .args([
            "--exact",
            "child_measures_one_shape",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("ADVERSARIAL_SHAPE", name)
        .env("ADVERSARIAL_BYTES", bytes.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a child process");
    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("child status") {
            Some(status) => break status,
            None if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::TimedOut;
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let Some(result) = stdout.lines().find_map(|line| line.strip_prefix("RESULT ")) else {
        let tail: String = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .chars()
            .take(120)
            .collect();
        return Outcome::Failed(format!("{status} {tail}"));
    };
    let fields: Vec<&str> = result.split_whitespace().collect();
    let nanos = |index: usize| Duration::from_nanos(fields[index].parse().expect("nanoseconds"));
    Outcome::Done {
        base: nanos(0),
        full: nanos(1),
        counts: format!("d{}/i{}/r{}", fields[2], fields[3], fields[4]),
    }
}

fn sizes() -> Vec<usize> {
    env::var("ADVERSARIAL_SIZES")
        .unwrap_or_else(|_| "64,128,256,512,1024".to_string())
        .split(',')
        .map(|kib| kib.trim().parse::<usize>().expect("sizes in KiB") << 10)
        .collect()
}

#[test]
#[ignore = "informational timing; run with --ignored --nocapture"]
fn print_growth_of_hostile_shapes_per_doubling() {
    let only: Option<Vec<String>> = env::var("ADVERSARIAL_ONLY")
        .ok()
        .map(|names| names.split(',').map(|name| name.trim().to_string()).collect());
    let timeout = Duration::from_secs(
        env::var("ADVERSARIAL_TIMEOUT_SECS")
            .ok()
            .and_then(|seconds| seconds.parse().ok())
            .unwrap_or(10),
    );
    let sizes = sizes();
    println!("cell = KiB base/full (growth of full per doubling); counts d=declarations i=invocations r=references");
    let mut suspects = Vec::new();
    for shape in SHAPES {
        if only
            .as_ref()
            .is_some_and(|names| !names.iter().any(|name| name == shape.name))
        {
            continue;
        }
        let mut cells = Vec::new();
        let mut previous: Option<Duration> = None;
        let mut last_growth = 0.0f64;
        let mut verdict = "ok";
        for &bytes in &sizes {
            let label = bytes >> 10;
            match measure(shape.name, bytes, timeout) {
                Outcome::Done { base, full, counts } => {
                    let growth = previous.map(|before| {
                        full.as_secs_f64() / before.as_secs_f64().max(1e-9)
                    });
                    let note = growth.map_or_else(String::new, |growth| {
                        if full > Duration::from_millis(40) {
                            last_growth = growth;
                        }
                        format!(" x{growth:.1}")
                    });
                    cells.push(format!("{label}K {base:.0?}/{full:.0?}{note} {counts}"));
                    previous = Some(full);
                }
                Outcome::TimedOut => {
                    cells.push(format!("{label}K TIMEOUT>{}s", timeout.as_secs()));
                    verdict = "SLOW";
                    break;
                }
                Outcome::Failed(reason) => {
                    cells.push(format!("{label}K FAILED {reason}"));
                    verdict = "FAILED";
                    break;
                }
            }
        }
        if verdict == "ok" && last_growth >= 3.0 {
            verdict = "QUADRATIC?";
        }
        if verdict != "ok" {
            suspects.push(format!("{} ({verdict})", shape.name));
        }
        println!("[{verdict}] {}: {}", shape.name, cells.join(" | "));
    }
    println!("suspects: {}", suspects.join(", "));
}
