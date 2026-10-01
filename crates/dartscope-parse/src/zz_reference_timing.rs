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
    println!(
        "phase {shape} n={n} bytes={} decls={} bindings={} regions={} refs={} file={t_file:?} facts={t_facts:?} regions_t={t_regions:?} bindings_t={t_bindings:?} identifiers={t_identifiers:?} reads={t_reads:?} writes={t_writes:?} updates={t_updates:?} methods={t_methods:?} properties={t_properties:?} operators={t_operators:?} sort={t_sort:?}",
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
