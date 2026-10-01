//! TEMPORARY differential digest tool (removed before hand-off).
#![allow(clippy::all, clippy::pedantic, unused)]
use std::panic::{self, AssertUnwindSafe};

use dartscope_core::DartFileInput;
use dartscope_parse::analyze_file_with_references;

const SEEDS: &[&str] = &[
    "\
import 'package:flutter/material.dart';
import 'src/util.dart' as util show helper hide other;
import 'src/stub.dart' if (dart.library.io) 'src/io.dart' deferred as lazy;
export 'src/api.dart' show Api;
part 'widget.g.dart';

/// A counter.
class Counter extends StatefulWidget {
  const Counter({super.key, required this.start, this.label = 'count'});

  final int start;
  final String label;

  @override
  State<Counter> createState() => _CounterState();
}

class _CounterState extends State<Counter> with SingleTickerProviderStateMixin {
  late int _value = widget.start;

  void _increment() {
    setState(() {
      _value += 1;
    });
  }

  @override
  Widget build(BuildContext context) {
    final label = '${widget.label}: $_value';
    return Padding(
      padding: const EdgeInsets.all(8),
      child: Text(label),
    );
  }
}
",
    "\
enum Planet with Describable implements Comparable<Planet> {
  mercury(0.39, 'Mercury'),
  venus(0.72, 'Venus'),
  earth(1, 'Earth');

  const Planet(this.distance, this.title);
  final double distance;
  final String title;

  String get summary => '$title at $distance AU';
  static Planet nearest() => values.first;

  @override
  int compareTo(Planet other) => distance.compareTo(other.distance);
}

mixin Describable on Object {
  String describe() => toString();
}

extension StringTools<T extends Object> on List<T> {
  T? get firstOrNull => isEmpty ? null : first;
  operator +(List<T> other) => [...this, ...other];
}

extension type Meters(int value) implements int {
  Meters.zero() : value = 0;
}
",
    "\
int get total => _total;
set total(int value) => _total = value;
int _total = 0;

typedef Callback<T> = void Function(T value);

Future<List<Map<String, int>>> load<T>(Callback<T> done, {int retries = 3}) async {
  for (var attempt = 0; attempt < retries; attempt++) {
    try {
      final result = await fetch(attempt).timeout(const Duration(seconds: 1));
      done(result as T);
      return [
        {'attempt': attempt, 'x': result.length},
        if (attempt > 0) {'again': 1},
        for (final extra in extras) {'extra': extra},
      ];
    } on TimeoutException catch (error, stack) {
      log('retry $attempt: ${error.message} ${stack}');
    } finally {
      cleanup()..flush()..close();
    }
  }
  return switch (retries) {
    0 => [],
    1 || 2 => [{'few': retries}],
    _ => throw StateError('too many'),
  };
}
",
    "\u{feff}library sample;\r\n\r\nimport 'dart:async';\r\n\r\n/// Привет, мир 😀\r\nclass Приветствие {\r\n  final String имя = 'мир ${1 + 2} 😀';\r\n  int значение(int x) => x * 2;\r\n}\r\n\r\nvoid main() {\r\n  var p = Приветствие();\r\n  print(p.значение(21));\r\n}\r\n",
    "\
@immutable
@Deprecated('use Other')
abstract class Shape<T extends Comparable<T>> implements Comparable<Shape<T>> {
  const Shape._();
  factory Shape.circle(double radius) = Circle;
  external int get id;
  double area();
  bool operator ==(Object other) => other is Shape && other.area() == area();
  @override
  int get hashCode => Object.hash(runtimeType, area());
}

class Circle extends Shape<num> {
  Circle(this.radius) : super._();
  final double radius;
  @override double area() => 3.14159 * radius * radius;
}

final shapes = <Shape<num>>[Circle(1), Circle(2)];
const names = {'circle': 'Circle', 'square': \"Square\"};
var text = r'raw $not ${interpolated}' '''triple
quoted''' \"\"\"another
one\"\"\";
",
    "\
part of 'main.dart';

void run() {
  var count = 0;
  final items = [1, 2, 3];
  items.map((item) {
    count += item;
    return count;
  }).toList();
  for (var i = 0, j = 10; i < j; i++, j--) {
    if (i.isEven) continue;
    count -= i;
  }
  var (a, b) = (1, 2);
  switch (count) {
    case 0:
      break;
    default:
      count = a + b;
  }
  label: while (true) {
    do {
      count++;
    } while (count < 5);
    break label;
  }
  callback(a: 1, b: count, c: (x) => x + 1);
}
",
    "\
class Matrix {
  final List<List<double>> _rows;
  Matrix(this._rows);
  Matrix.identity(int n) : _rows = List.generate(n, (i) => List.filled(n, i == 0 ? 1.0 : 0.0));
  Matrix operator *(Matrix other) {
    final result = Matrix.identity(_rows.length);
    for (var i = 0; i < _rows.length; i++) {
      for (var j = 0; j < other._rows[0].length; j++) {
        result._rows[i][j] = _rows[i].asMap().entries.fold(0.0, (sum, e) => sum + e.value * other._rows[e.key][j]);
      }
    }
    return result;
  }
  double operator [](int i) => _rows[i][0];
  void operator []=(int i, double v) => _rows[i][0] = v;
}
",
    "\
import 'package:graphql/client.dart';

const String viewerQuery = r'''
  query Viewer($id: ID!, $first: Int = 10) {
    viewer(id: $id) { name friends(first: $first) { edges { node { id } } } }
  }
''';

final listQuery = gql('''
  query List { items { id title } }
''');

Future<void> load(GraphQLClient client) async {
  final result = await client.query(QueryOptions(document: gql(viewerQuery), variables: {'id': '1'}));
  await client.mutate(MutationOptions(document: listQuery));
}
",
    "\
sealed class Event {}
base class Click extends Event { final (int x, int y) at; Click(this.at); }
final class Key extends Event { final String key; Key(this.key); }
interface class Port { void send(Object message); }
abstract mixin class Logging { void log(String m) => print(m); }

String describe(Event event) => switch (event) {
      Click(at: (var x, var y)) when x > 0 && y > 0 => 'click $x,$y',
      Click() => 'click',
      Key(:final key) => 'key $key',
    };

Stream<int> count(int to) async* {
  for (var i = 0; i < to; i++) {
    yield i;
  }
  yield* count(0);
}

void patterns(Object? value) {
  if (value case [int a, int b, ...final rest]) print('$a $b $rest');
  final {'name': name as String, 'age': int age} = {'name': 'x', 'age': 1};
  var (_, second) = (1, 2);
  late final int later;
  assert(second > 0, 'positive');
  outer:
  for (final e in [1, 2]) { continue outer; }
  value?.toString()..hashCode..runtimeType;
}
",
];

/// Fragments that change how much of the text is code, a string, a comment or a line break.
const TOKENS: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    ";",
    ",",
    "'",
    "\"",
    "'''",
    "\"\"\"",
    "/*",
    "*/",
    "//",
    "${",
    "$",
    "@",
    "\r",
    "\r\n",
    "\n",
    "\u{feff}",
    "é",
    "😀",
    "日本",
    "class ",
    "enum ",
    "extension ",
    " on ",
    " with ",
    " extends ",
    "=>",
    "=",
    "?",
    ":",
    "..",
    "?.",
    "!",
    "late ",
    "final ",
    "var ",
    "import '",
    "export '",
    "part '",
    "r'",
    "\\",
    "get ",
    "set ",
    "operator ",
    "factory ",
    "static ",
    "const ",
    "async ",
    "await ",
    "this.",
    "super.",
    "@override\n",
];

/// xorshift64*: small, deterministic and good enough to pick edit positions.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

fn mutate(rng: &mut Rng, source: &str) -> String {
    let mut chars: Vec<char> = source.chars().collect();
    for _ in 0..=rng.below(4) {
        if chars.is_empty() {
            chars.extend("class A {}".chars());
        }
        let at = rng.below(chars.len() + 1);
        match rng.below(5) {
            0 => {
                let end = (at + 1 + rng.below(12)).min(chars.len());
                let _ = chars.drain(at..end);
            }
            1 => {
                let token = TOKENS[rng.below(TOKENS.len())];
                let _ = chars.splice(at..at, token.chars());
            }
            2 => {
                let end = (at + 1 + rng.below(40)).min(chars.len());
                let piece: Vec<char> = chars[at.min(end)..end].to_vec();
                let target = rng.below(chars.len() + 1);
                let _ = chars.splice(target..target, piece);
            }
            3 => chars.truncate(at),
            _ => {
                if at < chars.len() {
                    let token = TOKENS[rng.below(TOKENS.len())];
                    let _ = chars.splice(at..=at, token.chars());
                }
            }
        }
    }
    chars.into_iter().collect()
}

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
            "class W{i} extends StatefulWidget {{\n  const W{i}({{super.key, required this.title}});\n  final String title;\n\n  @override\n  State<W{i}> createState() => _W{i}State();\n}}\n\nclass _W{i}State extends State<W{i}> {{\n  int _count = 0;\n  final List<String> _items = [];\n\n  void _increment(int step) {{\n    var total = _count + step;\n    for (var i = 0; i < step; i++) {{\n      total += i;\n    }}\n    setState(() {{\n      _count = total;\n      _items.add('item $total');\n    }});\n  }}\n\n  @override\n  Widget build(BuildContext context) {{\n    final label = '${{widget.title}}: $_count';\n    return Column(children: [\n      Text(label),\n      ..._items.map((item) => Text(item)),\n      ElevatedButton(onPressed: () => _increment(1), child: const Text('+')),\n    ]);\n  }}\n}}\n\n"
        ));
    }
    source
}

fn statements(n: usize) -> String {
    let mut source = String::from("class S {\n  int field = 0;\n  void run(int seed) {\n");
    for i in 0..n {
        source.push_str(&format!(
            "    var v{i} = seed + field;\n    v{i} += {i};\n    field = v{i};\n    print(v{i});\n    helper(v{i}, <int>[v{i}], a < b, c > d);\n"
        ));
    }
    source.push_str("  }\n  void helper(int a, List<int> b, bool c, bool d) {}\n}\n");
    source
}

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
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn main() {
    panic::set_hook(Box::new(|_| {}));
    let rounds = number("DIGEST_ROUNDS", 300);
    let salts = number("DIGEST_SALTS", 4);
    let args: Vec<String> = std::env::args().collect();
    let seeds = all_seeds();
    if let Some(position) = args.iter().position(|arg| arg == "--dump") {
        let id: Vec<usize> = args[position + 1]
            .split(':')
            .map(|part| part.parse().unwrap())
            .collect();
        let source = candidates(id[0], id[1], &seeds[id[1]], rounds).swap_remove(id[2]);
        println!("=== source ===\n{source}\n=== analysis ===");
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
