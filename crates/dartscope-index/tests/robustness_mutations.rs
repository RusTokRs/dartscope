//! Randomized equivalence test of the workspace index.
//!
//! An index that is updated one file at a time must end in the same state as an index built from
//! scratch, whatever the text of the files is: editors send broken code all the time. The test edits
//! two files with the same random damage a real session produces (see the parse crate's
//! `robustness_mutations.rs`), applies the edit through `upsert_file_with_references`, compares
//! every derived result of the snapshot with a stateless analysis, and then asks the resolution
//! context for the definition and the references of every reference. Neither the updates nor the
//! queries may panic.

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::{Mutex, PoisonError};

use dartscope_core::{DartFileInput, DartProjectInput, DartProjectReferenceAnalysis};
use dartscope_index::{
    DartDefinitionQuery, DartIndexOptions, DartWorkspaceIndex, DartWorkspaceResolutionContext,
    analyze_part_links, build_uri_graph_with_options, resolve_project_identifier_references_with_options,
};
use dartscope_parse::{analyze_file_with_references, analyze_project_with_references};

/// `(lib/a.dart, lib/b.dart)`: a client and the library it uses.
const PAIRS: &[(&str, &str)] = &[
    (
        "\
import 'b.dart';

class Child extends Base with Logging {
  int field = 1;

  void go(int arg) {
    var local = arg + helper(field);
    value = local;
    run();
    this.log('x');
    log('y');
    final text = 'abc'.twice();
    Base.make();
    print(Mode.fast);
    for (var i = 0; i < 3; i++) {
      local += i;
    }
    local++;
  }
}
",
        "\
class Base {
  int value = 0;
  void run() {}
  static int make() => 1;
}

mixin Logging {
  void log(String message) {}
}

extension StringTools on String {
  int get size => length;
  String twice() => this + this;
}

int helper(int x) => x + 1;

enum Mode { fast, slow }
",
    ),
    (
        "\
library lib_a;

part 'b.dart';

class Owner {
  void use() {
    inPart();
    FromPart().touch();
  }
}
",
        "\
part of 'a.dart';

void inPart() {}

class FromPart {
  void touch() {}
}
",
    ),
    (
        "\
import 'b.dart' show Foo hide Bar;
import 'b.dart' as prefixed;
export 'b.dart';

class UsesFoo extends Foo {
  prefixed.Bar bar = prefixed.Bar();
  void call() {
    hi();
    ext();
    bar.run();
  }
}
",
        "\
class Foo {
  void hi() {}
}

class Bar {
  void run() {}
}

extension on Foo {
  void ext() {}
}
",
    ),
    (
        "\
import 'b.dart';

int shadow = 1;

class Scopes {
  int shadow = 2;

  void run(int shadow, {int other = 3}) {
    final list = [for (var shadow in values) shadow + other];
    list.forEach((item) {
      var shadow = item;
      shadow += other;
    });
    try {
      other;
    } on Object catch (error, stack) {
      print(error);
    }
  }
}
",
        "\
final values = [1, 2, 3];
int shadow = 0;
void top() {}
",
    ),
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

/// Where and why a run panicked, recorded by the panic hook of this test binary.
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

fn project(a: &str, b: &str) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        vec![
            DartFileInput::new("lib/a.dart", a),
            DartFileInput::new("lib/b.dart", b),
        ],
        vec![],
    ))
}

/// Updates an index built from `(a0, b0)` to `(a1, b1)`, one file at a time, and compares it with
/// an index built from `(a1, b1)`. The name of the first component that differs, if any.
fn divergence(a0: &str, b0: &str, a1: &str, b1: &str) -> Option<&'static str> {
    let options = DartIndexOptions::default();
    let mut index = DartWorkspaceIndex::from_reference_project(project(a0, b0));
    if a1 != a0 {
        let _ = index.upsert_file_with_references(analyze_file_with_references(
            DartFileInput::new("lib/a.dart", a1),
        ));
    }
    if b1 != b0 {
        let _ = index.upsert_file_with_references(analyze_file_with_references(
            DartFileInput::new("lib/b.dart", b1),
        ));
    }
    let fresh = project(a1, b1);
    let snapshot = index.snapshot();
    if snapshot.project() != &fresh.project {
        return Some("project");
    }
    if snapshot.uri_graph() != &build_uri_graph_with_options(&fresh.project, &options) {
        return Some("uri graph");
    }
    if snapshot.part_links() != &analyze_part_links(&fresh.project) {
        return Some("part links");
    }
    if snapshot.identifier_reference_resolutions()
        != &resolve_project_identifier_references_with_options(&fresh, &options)
    {
        return Some("reference resolutions");
    }

    let context = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
    for reference in &fresh.references {
        let query =
            DartDefinitionQuery::new(reference.source_path.clone(), reference.span.byte_start);
        for resolution in context.find_definitions(&[query]).resolutions {
            let _ = context.find_references(&resolution.targets);
        }
    }
    None
}

/// What running one edit produced: where it panicked, or how the index diverged.
enum Failure {
    Panic(String, String),
    Divergence(&'static str),
}

fn run(a0: &str, b0: &str, a1: &str, b1: &str) -> Option<Failure> {
    PANICS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    match panic::catch_unwind(AssertUnwindSafe(|| divergence(a0, b0, a1, b1))) {
        Ok(Some(what)) => Some(Failure::Divergence(what)),
        Ok(None) => None,
        Err(_) => {
            let (place, message) = PANICS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .first()
                .cloned()
                .unwrap_or_else(|| ("unknown location".to_string(), String::new()));
            Some(Failure::Panic(place, message))
        }
    }
}

/// Removes characters from `source` for as long as `fails` stays true of the rest.
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

struct Found {
    count: usize,
    message: String,
    pair: usize,
    a: String,
    b: String,
}

fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn an_updated_index_equals_a_fresh_one_and_never_panics_on_broken_code() {
    // The defaults keep the test fast; a hunt for rare failures turns both knobs up.
    let rounds = env_number("DARTSCOPE_MUTATION_ROUNDS", 40);
    let salt = env_number("DARTSCOPE_MUTATION_SEED", 0) as u64;
    panic::set_hook(Box::new(record_panic));
    let mut runs = 0usize;
    let mut found: BTreeMap<String, Found> = BTreeMap::new();
    for (pair, (a0, b0)) in PAIRS.iter().enumerate() {
        let mut rng = Rng(
            0x9E37_79B9_7F4A_7C15
                ^ ((pair as u64 + 1) * 0x1000_0000_01B3)
                ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93),
        );
        for _ in 0..rounds {
            let a1 = mutate(&mut rng, a0);
            let b1 = if rng.below(3) == 0 {
                mutate(&mut rng, b0)
            } else {
                (*b0).to_string()
            };
            runs += 1;
            let Some(failure) = run(a0, b0, &a1, &b1) else {
                continue;
            };
            let (key, message) = match failure {
                Failure::Panic(place, message) => (format!("panic at {place}"), message),
                Failure::Divergence(what) => (format!("divergence in {what}"), String::new()),
            };
            let entry = found.entry(key).or_insert_with(|| Found {
                count: 0,
                message,
                pair,
                a: a1.clone(),
                b: b1.clone(),
            });
            entry.count += 1;
            if a1.len() + b1.len() < entry.a.len() + entry.b.len() {
                entry.pair = pair;
                entry.a = a1;
                entry.b = b1;
            }
        }
    }
    // The same failure on the smallest texts: the edited file first, then the library.
    for (key, entry) in &mut found {
        let (a0, b0) = PAIRS[entry.pair];
        let same = |failure: Option<Failure>| match failure {
            Some(Failure::Panic(place, _)) => format!("panic at {place}") == *key,
            Some(Failure::Divergence(what)) => format!("divergence in {what}") == *key,
            None => false,
        };
        let b = entry.b.clone();
        entry.a = shrink(&entry.a, &|candidate| same(run(a0, b0, candidate, &b)));
        let a = entry.a.clone();
        entry.b = shrink(&entry.b, &|candidate| same(run(a0, b0, &a, candidate)));
    }
    drop(panic::take_hook());

    let report: Vec<String> = found
        .iter()
        .map(|(key, entry)| {
            format!(
                "{key} x{}: {:.80} -- pair {} a1 {:?} b1 {:?}",
                entry.count,
                entry.message.replace('\n', " "),
                entry.pair,
                entry.a,
                entry.b
            )
        })
        .collect();
    assert!(report.is_empty(), "\n{}", report.join("\n"));
    assert!(runs >= 150, "only {runs} edits were run");
}
