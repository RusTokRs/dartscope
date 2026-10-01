//! TEMPORARY development-loop measurement (removed before hand-off): which reference pass is
//! superlinear in the size of one file, for several realistic shapes.
use std::time::Instant;

use dartscope_core::DartFileInput;

use crate::file_facts::FileFacts;
use crate::identifier_references::{collect_identifier_references, sort_identifier_references};
use crate::lexical::mask_non_code;
use crate::lexical_bindings::collect_lexical_bindings;
use crate::lexical_reads::collect_lexical_read_references;
use crate::lexical_regions::analyze_lexical_regions;
use crate::lexical_writes::{collect_lexical_update_references, collect_lexical_write_references};
use crate::member_references::collect_method_references;
use crate::operator_references::collect_operator_references;
use crate::property_references::collect_property_references;
use crate::source_lines::LineIndexScope;

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

fn widgets(n: usize) -> String {
    let mut source = String::from("import 'package:flutter/material.dart';\n\n");
    for i in 0..n {
        source.push_str(&format!(
            "class W{i} extends StatefulWidget {{
  const W{i}({{super.key, required this.title}});
  final String title;

  @override
  State<W{i}> createState() => _W{i}State();
}}

class _W{i}State extends State<W{i}> {{
  int _count = 0;
  final List<String> _items = [];

  void _increment(int step) {{
    var total = _count + step;
    for (var i = 0; i < step; i++) {{
      total += i;
    }}
    setState(() {{
      _count = total;
      _items.add('item $total');
    }});
  }}

  @override
  Widget build(BuildContext context) {{
    final label = '${{widget.title}}: $_count';
    return Column(children: [
      Text(label),
      ..._items.map((item) => Text(item)),
      ElevatedButton(onPressed: () => _increment(1), child: const Text('+')),
    ]);
  }}
}}

"
        ));
    }
    source
}

/// One method with `n` statements that use locals, assignments and member reads.
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

/// One `build` method that returns one nested expression with `n` children.
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

/// `n` top-level functions with arrow bodies that call their predecessor.
fn functions(n: usize) -> String {
    let mut source = String::from("int f0(int a) => a;\n");
    for i in 1..n {
        source.push_str(&format!("int f{i}(int a) => f{}(a) + a;\n", i - 1));
    }
    source
}

/// One method with `n` blocks that each declare the same local name and use it.
fn same_name_locals(n: usize) -> String {
    let mut source = String::from("class L {\n  void run(int seed) {\n");
    for i in 0..n {
        source.push_str(&format!(
            "    {{\n      var tmp = seed + {i};\n      print(tmp);\n    }}\n"
        ));
    }
    source.push_str("  }\n}\n");
    source
}

/// One method with `n` closures and `for` loops, which bind closure parameters and loop variables.
fn closures(n: usize) -> String {
    let mut source = String::from("class C {\n  void run(List<int> items) {\n");
    for i in 0..n {
        source.push_str(&format!(
            "    items.map((x) => x + {i}).toList();\n    items.forEach((y) {{ print(y); }});\n    for (var i = 0; i < {i}; i++) {{ print(i); }}\n    for (final z in items) {{ print(z); }}\n"
        ));
    }
    source.push_str("  }\n}\n");
    source
}

/// One expression with `n` identifier terms.
fn long_expression(n: usize) -> String {
    let mut source = String::from("int run(int a) {\n  return a");
    for _ in 0..n {
        source.push_str(" + a");
    }
    source.push_str(";\n}\n");
    source
}

/// `n` imports with prefixes, each used once.
fn imports(n: usize) -> String {
    let mut source = String::new();
    for i in 0..n {
        source.push_str(&format!("import 'package:a/a{i}.dart' as p{i};\n"));
    }
    source.push_str("void run() {\n");
    for i in 0..n {
        source.push_str(&format!("  p{i}.Thing.make();\n  p{i}.go();\n"));
    }
    source.push_str("}\n");
    source
}

/// A very long map literal and list of constructor calls with named arguments.
fn literals(n: usize) -> String {
    let mut source = String::from("const Map<String, int> table = {\n");
    for i in 0..n {
        source.push_str(&format!("  'k{i}': {i},\n"));
    }
    source.push_str("};\nfinal items = [\n");
    for i in 0..n {
        source.push_str(&format!("  Item(id: {i}, name: 'n{i}', tags: const <String>['a']),\n"));
    }
    source.push_str("];\n");
    source
}

/// Nested parentheses, which unbalanced or deeply nested code can make expensive to match.
fn deep_parens(n: usize) -> String {
    let mut source = String::from("void run() {\n  f");
    for _ in 0..n {
        source.push_str("(g");
    }
    for _ in 0..n {
        source.push(')');
    }
    source.push_str(";\n}\n");
    source
}

/// `n` opening parentheses that are never closed.
fn unbalanced(n: usize) -> String {
    let mut source = String::from("void run() {\n");
    for i in 0..n {
        source.push_str(&format!("  f{i}(a, b {{ \n"));
    }
    source
}

fn measure(shape: &str, n: usize, source: &str) {
    let t = Instant::now();
    let file = crate::analyze_file(DartFileInput::new("lib/a.dart", source.to_string()));
    let t_file = t.elapsed();
    let _lines = LineIndexScope::enter(source);
    let lexical = mask_non_code(source);
    let t = Instant::now();
    let facts = FileFacts::new(&lexical.code, &file);
    let t_facts = t.elapsed();
    let t = Instant::now();
    let regions = analyze_lexical_regions(&lexical.code, &file, &facts.tables);
    let t_regions = t.elapsed();
    let t = Instant::now();
    let bindings = collect_lexical_bindings(source, &lexical.code, &file, &facts);
    let t_bindings = t.elapsed();
    let t = Instant::now();
    let mut references =
        collect_identifier_references(source, &lexical.code, &file, &facts, &bindings);
    let t_identifiers = t.elapsed();
    let t = Instant::now();
    let reads = collect_lexical_read_references(
        source,
        &lexical.code,
        &file,
        &facts,
        &bindings,
        &references,
    );
    references.extend(reads);
    let t_reads = t.elapsed();
    let t = Instant::now();
    let writes = collect_lexical_write_references(
        source,
        &lexical.code,
        &file,
        &facts,
        &bindings,
        &references,
    );
    references.extend(writes);
    let t_writes = t.elapsed();
    let t = Instant::now();
    let updates = collect_lexical_update_references(
        source,
        &lexical.code,
        &file,
        &facts,
        &bindings,
        &references,
    );
    references.extend(updates);
    let t_updates = t.elapsed();
    let t = Instant::now();
    references.extend(collect_method_references(
        source,
        &lexical.code,
        &file,
        &facts,
        &bindings,
    ));
    let t_methods = t.elapsed();
    let t = Instant::now();
    references.extend(collect_property_references(
        source,
        &lexical.code,
        &file,
        &facts,
        &bindings,
    ));
    let t_properties = t.elapsed();
    let t = Instant::now();
    references.extend(collect_operator_references(
        source,
        &lexical.code,
        &file,
        &facts,
    ));
    let t_operators = t.elapsed();
    let t = Instant::now();
    sort_identifier_references(&mut references);
    let t_sort = t.elapsed();
    let total = t_file + t_facts + t_regions + t_bindings + t_identifiers + t_reads + t_writes
        + t_updates + t_methods + t_properties + t_operators + t_sort;
    println!(
        "phase {shape} n={n} total={total:?} bytes={} decls={} bindings={} regions={} refs={} file={t_file:?} facts={t_facts:?} regions_t={t_regions:?} bindings_t={t_bindings:?} identifiers={t_identifiers:?} reads={t_reads:?} writes={t_writes:?} updates={t_updates:?} methods={t_methods:?} properties={t_properties:?} operators={t_operators:?} sort={t_sort:?}",
        source.len(),
        file.declarations.len(),
        bindings.len(),
        regions.deferred_regions.len(),
        references.len()
    );
}

#[test]
#[ignore = "development-loop measurement"]
fn zz_reference_timing() {
    let only = std::env::var("ZZ_SHAPE").unwrap_or_default();
    let run = |name: &str| only.is_empty() || only == name;
    if run("classes") {
        for n in [500usize, 1000, 2000, 4000] {
            measure("classes", n, &classes(n));
        }
    }
    if run("widgets") {
        for n in [100usize, 200, 400, 800] {
            measure("widgets", n, &widgets(n));
        }
    }
    if run("statements") {
        for n in [1000usize, 2000, 4000, 8000] {
            measure("statements", n, &statements(n));
        }
    }
    if run("tree") {
        for n in [1000usize, 2000, 4000, 8000] {
            measure("tree", n, &tree(n));
        }
    }
}

#[test]
#[ignore = "development-loop measurement"]
fn zz_reference_timing_more() {
    for n in [4000usize, 8000, 16000] {
        measure("functions", n, &functions(n));
        measure("same_name_locals", n, &same_name_locals(n));
        measure("closures", n / 2, &closures(n / 2));
        measure("long_expression", n, &long_expression(n));
        measure("imports", n / 2, &imports(n / 2));
        measure("literals", n, &literals(n));
    }
    for n in [1000usize, 2000, 4000] {
        measure("deep_parens", n, &deep_parens(n));
        measure("unbalanced", n, &unbalanced(n));
    }
}
