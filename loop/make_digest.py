#!/usr/bin/env python3
"""TEMPORARY (removed before hand-off): writes the differential digest tool into one or more trees.

The tool replays the mutation corpus of crates/dartscope-parse/tests/robustness_mutations.rs and prints a
fingerprint of the complete analysis of every mutant, so that two builds can be compared byte for byte.
"""
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
test = (root / "crates/dartscope-parse/tests/robustness_mutations.rs").read_text()
begin = test.index("const SEEDS: &[&str] = &[")
end = test.index("/// The one-based line and column of a byte offset")
corpus = test[begin:end]

tool = '''//! TEMPORARY differential digest tool (removed before hand-off).
#![allow(clippy::all, clippy::pedantic, unused)]
use std::panic::{self, AssertUnwindSafe};

use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file_with_references;

''' + corpus + '''
fn fnv(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn classes(n: usize) -> String {
    let mut source = String::new();
    for class in 0..n {
        let base = if class > 0 { format!("C{}", class - 1) } else { "Object".to_string() };
        source.push_str(&format!(
            "class C{class} extends {base} {{\\n  int a{class} = 0;\\n  int m{class}(int x) {{\\n    this.a{class} = x;\\n    return this.m0(x) + this.a{class};\\n  }}\\n  void run{class}() {{\\n    this.run0();\\n    this.m{class}(1);\\n    this.missing();\\n    helper();\\n  }}\\n}}\\n"
        ));
    }
    source.push_str("int helper() => 1;\\n");
    source
}

fn widgets(n: usize) -> String {
    let mut source = String::from("import 'package:flutter/material.dart';\\n\\n");
    for i in 0..n {
        source.push_str(&format!(
            "class W{i} extends StatefulWidget {{\\n  const W{i}({{super.key, required this.title}});\\n  final String title;\\n\\n  @override\\n  State<W{i}> createState() => _W{i}State();\\n}}\\n\\nclass _W{i}State extends State<W{i}> {{\\n  int _count = 0;\\n  final List<String> _items = [];\\n\\n  void _increment(int step) {{\\n    var total = _count + step;\\n    for (var i = 0; i < step; i++) {{\\n      total += i;\\n    }}\\n    setState(() {{\\n      _count = total;\\n      _items.add('item $total');\\n    }});\\n  }}\\n\\n  @override\\n  Widget build(BuildContext context) {{\\n    final label = '${{widget.title}}: $_count';\\n    return Column(children: [\\n      Text(label),\\n      ..._items.map((item) => Text(item)),\\n      ElevatedButton(onPressed: () => _increment(1), child: const Text('+')),\\n    ]);\\n  }}\\n}}\\n\\n"
        ));
    }
    source
}

fn statements(n: usize) -> String {
    let mut source = String::from("class S {\\n  int field = 0;\\n  void run(int seed) {\\n");
    for i in 0..n {
        source.push_str(&format!(
            "    var v{i} = seed + field;\\n    v{i} += {i};\\n    field = v{i};\\n    print(v{i});\\n    helper(v{i}, <int>[v{i}], a < b, c > d);\\n"
        ));
    }
    source.push_str("  }\\n  void helper(int a, List<int> b, bool c, bool d) {}\\n}\\n");
    source
}

fn tree(n: usize) -> String {
    let mut source = String::from(
        "class T extends StatelessWidget {\\n  final String title = 'x';\\n  Widget build(BuildContext context) {\\n    return Column(children: [\\n",
    );
    for i in 0..n {
        source.push_str(&format!(
            "      Padding(padding: EdgeInsets.all({i}), child: Text(title + '{i}')),\\n"
        ));
    }
    source.push_str("    ]);\\n  }\\n}\\n");
    source
}

fn all_seeds() -> Vec<String> {
    let mut seeds: Vec<String> = SEEDS.iter().map(|seed| (*seed).to_string()).collect();
    seeds.push(classes(4));
    seeds.push(widgets(3));
    seeds.push(statements(6));
    seeds.push(tree(8));
    seeds
}

fn candidates(salt: usize, index: usize, seed: &str, rounds: usize) -> Vec<String> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15
        ^ ((index as u64 + 1) * 0x1000_0000_01B3)
        ^ (salt as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let mut result = vec![seed.to_string()];
    result.extend((0..rounds).map(|_| mutate(&mut rng, seed)));
    result
}

fn analyze(source: &str) -> Result<String, ()> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        let analysis =
            analyze_file_with_references(DartFileInput::new("lib/a.dart", source.to_string()));
        format!("{analysis:#?}")
    }))
    .map_err(|_| ())
}

fn number(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn main() {
    panic::set_hook(Box::new(|_| {}));
    let rounds = number("DIGEST_ROUNDS", 300);
    let salts = number("DIGEST_SALTS", 4);
    let args: Vec<String> = std::env::args().collect();
    let seeds = all_seeds();
    if let Some(position) = args.iter().position(|arg| arg == "--dump") {
        let id: Vec<usize> = args[position + 1].split(':').map(|part| part.parse().unwrap()).collect();
        let source = candidates(id[0], id[1], &seeds[id[1]], rounds).swap_remove(id[2]);
        println!("=== source ===\\n{source}\\n=== analysis ===");
        match analyze(&source) {
            Ok(text) => println!("{text}"),
            Err(()) => println!("PANIC"),
        }
        return;
    }
    for salt in 0..salts {
        for (index, seed) in seeds.iter().enumerate() {
            for (n, source) in candidates(salt, index, seed, rounds).iter().enumerate() {
                match analyze(source) {
                    Ok(text) => println!("{salt}:{index}:{n}:{:016x}", fnv(&text)),
                    Err(()) => println!("{salt}:{index}:{n}:PANIC"),
                }
            }
        }
    }
}
'''
for target in sys.argv[2:]:
    path = pathlib.Path(target) / "crates/dartscope-parse/examples/zz_digest.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(tool)
    print("wrote", path)
