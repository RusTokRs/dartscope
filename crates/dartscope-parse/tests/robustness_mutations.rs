//! Deterministic mutation test of the whole reference analysis.
//!
//! Real Dart files are damaged in many small ways (deleted, duplicated and inserted fragments, cut
//! off files, stray delimiters, byte-order marks, CR and CRLF line ends, non-ASCII text), and the
//! analysis of every result must neither panic nor report a span that does not describe the text:
//! offsets inside the source and on character boundaries, ordered lines, and line and column numbers
//! that agree with an independent count.

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use dartscope_core::{DartFileInput, DartFileReferenceAnalysis, SourceSpan};
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

/// The one-based line and column of a byte offset, counted independently of the analysis: lines end
/// at `\n`, a leading byte-order mark is not part of any line, and columns count characters.
fn expected_line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let preamble = if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    if offset < preamble {
        return (1, 1);
    }
    let before = &source[preamble..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(preamble, |at| preamble + at + 1);
    (line, source[line_start..offset].chars().count() + 1)
}

fn all_spans(analysis: &DartFileReferenceAnalysis) -> Vec<(&'static str, &SourceSpan)> {
    let file = &analysis.file;
    let mut spans = Vec::new();
    for declaration in &file.declarations {
        spans.push(("declaration", &declaration.span));
        spans.extend(
            declaration
                .declaration_span
                .iter()
                .map(|span| ("declaration_span", span)),
        );
    }
    for invocation in &file.invocations {
        spans.push(("invocation", &invocation.span));
        spans.push(("invocation line", &invocation.source_line_span));
        spans.extend(
            invocation
                .arguments
                .iter()
                .map(|argument| ("argument", &argument.span)),
        );
    }
    spans.extend(file.imports.iter().map(|import| ("import", &import.span)));
    spans.extend(file.exports.iter().map(|export| ("export", &export.span)));
    spans.extend(file.parts.iter().map(|part| ("part", &part.span)));
    spans.extend(
        file.string_constants
            .iter()
            .map(|constant| ("string constant", &constant.span)),
    );
    spans.extend(
        file.diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.span.as_ref())
            .map(|span| ("diagnostic", span)),
    );
    spans.extend(
        analysis
            .references
            .iter()
            .map(|reference| ("reference", &reference.span)),
    );
    for binding in &analysis.bindings {
        spans.push(("binding declaration", &binding.declaration_span));
        spans.push(("binding scope", &binding.scope_span));
    }
    spans
}

/// Everything that is wrong with the spans of `analysis` of `source`.
fn span_problems(source: &str, analysis: &DartFileReferenceAnalysis) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut problems = Vec::new();
    for (what, span) in all_spans(analysis) {
        let (start, end) = (span.byte_start, span.byte_end);
        if start > end || end > source.len() {
            problems.push(format!("{what}: bytes {start}..{end} of {}", source.len()));
            continue;
        }
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            problems.push(format!("{what}: bytes {start}..{end} are not boundaries"));
            continue;
        }
        if span.start_line == 0 || span.start_column == 0 || span.end_line < span.start_line {
            problems.push(format!("{what}: bad lines or columns in {span:?}"));
            continue;
        }
        // An offset inside a CRLF pair, or at the very end of the text, has no position of its own.
        let has_position = |offset: usize| {
            offset < source.len()
                && !(offset > 0 && bytes[offset - 1] == b'\r' && bytes[offset] == b'\n')
        };
        for (offset, line, column) in [
            (start, span.start_line, span.start_column),
            (end, span.end_line, span.end_column),
        ] {
            if has_position(offset) && expected_line_and_column(source, offset) != (line, column) {
                problems.push(format!(
                    "{what}: byte {offset} is at {:?}, the span says {:?} ({span:?})",
                    expected_line_and_column(source, offset),
                    (line, column)
                ));
            }
        }
    }
    problems
}

/// Where and why the analysis panicked, recorded by the panic hook of this test binary.
static PANICS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn record_panic(info: &PanicHookInfo<'_>) {
    let place = info.location().map_or_else(
        || "unknown location".to_string(),
        |at| format!("{}:{}", at.file(), at.line()),
    );
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    PANICS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((place, message));
}

/// Analyzes `source`, or returns where the analysis panicked and what it said.
fn try_analyze(source: &str) -> Result<DartFileReferenceAnalysis, (String, String)> {
    PANICS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    panic::catch_unwind(AssertUnwindSafe(|| {
        analyze_file_with_references(DartFileInput::new("lib/a.dart", source))
    }))
    .map_err(|_| {
        PANICS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first()
            .cloned()
            .unwrap_or_else(|| ("unknown location".to_string(), String::new()))
    })
}

/// The kind of span a problem is about: the text of the problem up to its first colon.
fn problem_kind(problem: &str) -> String {
    problem.split(':').next().unwrap_or_default().to_string()
}

/// Removes characters from `source` for as long as `fails` stays true of the rest, so that a
/// failure is reported with a source of a few characters instead of a whole mutated file.
fn shrink(source: &str, fails: &dyn Fn(&str) -> bool) -> String {
    let mut chars: Vec<char> = source.chars().collect();
    let mut budget = 1500usize;
    let mut chunk = chars.len().div_ceil(2).max(1);
    loop {
        let mut index = 0;
        while index < chars.len() && budget > 0 {
            let end = (index + chunk).min(chars.len());
            let candidate: String = chars[..index].iter().chain(&chars[end..]).collect();
            budget -= 1;
            if fails(&candidate) {
                let _ = chars.drain(index..end);
            } else {
                index += chunk;
            }
        }
        if chunk == 1 || budget == 0 {
            break;
        }
        chunk = chunk.div_ceil(2);
    }
    chars.into_iter().collect()
}

/// One kind of failure: how often it happened, what it said, and the smallest source seen.
struct Found {
    count: usize,
    message: String,
    source: String,
}

fn note(found: &mut BTreeMap<String, Found>, key: String, message: String, source: &str) {
    let entry = found.entry(key).or_insert_with(|| Found {
        count: 0,
        message,
        source: source.to_string(),
    });
    entry.count += 1;
    if source.len() < entry.source.len() {
        entry.source = source.to_string();
    }
}

fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn mutated_sources_never_panic_and_report_spans_that_describe_the_text() {
    // The defaults keep the test fast; a hunt for rare failures turns both knobs up.
    let rounds = env_number("DARTSCOPE_MUTATION_ROUNDS", 500);
    let salt = env_number("DARTSCOPE_MUTATION_SEED", 0) as u64;
    panic::set_hook(Box::new(record_panic));
    let mut analyzed = 0usize;
    let mut panics = BTreeMap::new();
    let mut problems = BTreeMap::new();
    for (index, seed) in SEEDS.iter().enumerate() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15
            ^ ((index as u64 + 1) * 0x1000_0000_01B3)
            ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93));
        let mut candidates = vec![(*seed).to_string()];
        candidates.extend((0..rounds).map(|_| mutate(&mut rng, seed)));
        for source in candidates {
            analyzed += 1;
            let started = Instant::now();
            match try_analyze(&source) {
                Err((place, message)) => note(&mut panics, place, message, &source),
                Ok(analysis) => {
                    for problem in span_problems(&source, &analysis) {
                        note(&mut problems, problem_kind(&problem), problem, &source);
                    }
                }
            }
            if started.elapsed() > Duration::from_secs(2) {
                let message = format!("{:?} for {} bytes", started.elapsed(), source.len());
                note(&mut problems, "slow analysis".to_string(), message, &source);
            }
        }
    }
    for (place, found) in &mut panics {
        let place = place.clone();
        found.source = shrink(
            &found.source,
            &|candidate| matches!(try_analyze(candidate), Err((at, _)) if at == place),
        );
    }
    for (kind, found) in &mut problems {
        let kind = kind.clone();
        found.source = shrink(&found.source, &|candidate| {
            try_analyze(candidate).is_ok_and(|analysis| {
                span_problems(candidate, &analysis)
                    .iter()
                    .any(|problem| problem_kind(problem) == kind)
            })
        });
    }
    drop(panic::take_hook());

    let mut report = Vec::new();
    for (place, found) in &panics {
        report.push(format!(
            "panic at {place} x{}: {:.100} -- reproducer {:?}",
            found.count,
            found.message.replace('\n', " "),
            found.source
        ));
    }
    for (kind, found) in &problems {
        report.push(format!(
            "span problem `{kind}` x{}: {:.160} -- reproducer {:?}",
            found.count,
            found.message.replace('\n', " "),
            found.source
        ));
    }
    assert!(report.is_empty(), "\n{}", report.join("\n"));
    assert!(analyzed >= 3000, "only {analyzed} sources were analyzed");
}
