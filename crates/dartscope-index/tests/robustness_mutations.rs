//! Randomized equivalence test of the workspace index.
//!
//! An index that is updated one file at a time must end in the same state as an index built from
//! scratch, whatever the text of the files is: editors send broken code all the time. The test
//! applies short random sequences of edits (the same random damage a real session produces, see the
//! parse crate's `robustness_mutations.rs`), removals and re-additions of the two files of a pair
//! through the incremental API, compares every derived result of the snapshot with a stateless
//! analysis after each step, and asks the resolution context for the definition and the references
//! of every reference. Neither the updates nor the queries may panic.

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::{Mutex, PoisonError};

use dartscope_core::{DartFileInput, DartProjectInput, DartProjectReferenceAnalysis};
use dartscope_index::{
    DartDefinitionQuery, DartIndexOptions, DartWorkspaceIndex, DartWorkspaceResolutionContext,
    analyze_part_links, build_uri_graph_with_options,
    resolve_project_identifier_references_with_options,
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

const FILES: [&str; 2] = ["lib/a.dart", "lib/b.dart"];

/// One change of the workspace.
#[derive(Clone, Debug)]
enum Step {
    Edit(usize, String),
    Remove(usize),
    Add(usize, String),
}

impl Step {
    fn with_text(&self, text: &str) -> Step {
        match self {
            Step::Edit(file, _) => Step::Edit(*file, text.to_string()),
            Step::Add(file, _) => Step::Add(*file, text.to_string()),
            Step::Remove(file) => Step::Remove(*file),
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Step::Edit(_, text) | Step::Add(_, text) => Some(text),
            Step::Remove(_) => None,
        }
    }

    fn describe(&self) -> String {
        let name = |file: &usize| if *file == 0 { "a" } else { "b" };
        match self {
            Step::Edit(file, text) => format!("edit {} {:?}", name(file), text),
            Step::Add(file, text) => format!("add {} {:?}", name(file), text),
            Step::Remove(file) => format!("remove {}", name(file)),
        }
    }
}

fn project_of(files: &[Option<String>; 2]) -> DartProjectReferenceAnalysis {
    analyze_project_with_references(DartProjectInput::new(
        ".",
        FILES
            .iter()
            .zip(files)
            .filter_map(|(path, text)| text.as_ref().map(|text| DartFileInput::new(*path, text.as_str())))
            .collect(),
        vec![],
    ))
}

/// Applies `steps` to an index built from `initial` and compares it with a fresh analysis after every
/// step. The number of the first step that diverges and the component that differs, if any.
fn divergence(initial: (&str, &str), steps: &[Step]) -> Option<(usize, &'static str)> {
    let options = DartIndexOptions::default();
    let mut files = [Some(initial.0.to_string()), Some(initial.1.to_string())];
    let mut index = DartWorkspaceIndex::from_reference_project(project_of(&files));
    for (number, step) in steps.iter().enumerate() {
        match step {
            Step::Edit(file, text) | Step::Add(file, text) => {
                files[*file] = Some(text.clone());
                let _ = index.upsert_file_with_references(analyze_file_with_references(
                    DartFileInput::new(FILES[*file], text.as_str()),
                ));
            }
            Step::Remove(file) => {
                files[*file] = None;
                let _ = index.remove_file(FILES[*file]);
            }
        }
        let fresh = project_of(&files);
        let snapshot = index.snapshot();
        if snapshot.project() != &fresh.project {
            return Some((number, "project"));
        }
        if snapshot.uri_graph() != &build_uri_graph_with_options(&fresh.project, &options) {
            return Some((number, "uri graph"));
        }
        if snapshot.part_links() != &analyze_part_links(&fresh.project) {
            return Some((number, "part links"));
        }
        if snapshot.identifier_reference_resolutions()
            != &resolve_project_identifier_references_with_options(&fresh, &options)
        {
            return Some((number, "reference resolutions"));
        }

        let context = DartWorkspaceResolutionContext::from_snapshot(&snapshot);
        for reference in &fresh.references {
            let query =
                DartDefinitionQuery::new(reference.source_path.clone(), reference.span.byte_start);
            for resolution in context.find_definitions(&[query]).resolutions {
                let _ = context.find_references(&resolution.targets);
            }
        }
    }
    None
}

/// What running a sequence produced: where it panicked, or how the index diverged.
enum Failure {
    Panic(String, String),
    Divergence(usize, &'static str),
}

impl Failure {
    fn key(&self) -> String {
        match self {
            Failure::Panic(place, _) => format!("panic at {place}"),
            Failure::Divergence(_, what) => format!("divergence in {what}"),
        }
    }
}

fn run(initial: (&str, &str), steps: &[Step]) -> Option<Failure> {
    PANICS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    match panic::catch_unwind(AssertUnwindSafe(|| divergence(initial, steps))) {
        Ok(Some((number, what))) => Some(Failure::Divergence(number, what)),
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

/// A random sequence of one to six changes, each one applied to the text the previous ones left.
fn generate(rng: &mut Rng, initial: (&str, &str)) -> Vec<Step> {
    let seeds = [initial.0, initial.1];
    let mut current = [Some(initial.0.to_string()), Some(initial.1.to_string())];
    let mut steps = Vec::new();
    for _ in 0..=rng.below(6) {
        let present: Vec<usize> = (0..2).filter(|file| current[*file].is_some()).collect();
        let absent: Vec<usize> = (0..2).filter(|file| current[*file].is_none()).collect();
        let roll = rng.below(10);
        let step = if roll < 6 && !present.is_empty() {
            let file = present[rng.below(present.len())];
            let text = mutate(rng, current[file].as_deref().unwrap_or(seeds[file]));
            Step::Edit(file, text)
        } else if roll < 8 && !present.is_empty() {
            Step::Remove(present[rng.below(present.len())])
        } else if !absent.is_empty() {
            let file = absent[rng.below(absent.len())];
            let text = if rng.below(2) == 0 {
                seeds[file].to_string()
            } else {
                mutate(rng, seeds[file])
            };
            Step::Add(file, text)
        } else {
            let file = rng.below(2);
            Step::Edit(file, mutate(rng, current[file].as_deref().unwrap_or(seeds[file])))
        };
        match &step {
            Step::Edit(file, text) | Step::Add(file, text) => current[*file] = Some(text.clone()),
            Step::Remove(file) => current[*file] = None,
        }
        steps.push(step);
    }
    steps
}

/// Removes characters from `source` for as long as `fails` stays true of the rest.
fn shrink(source: &str, fails: &dyn Fn(&str) -> bool) -> String {
    let mut chars: Vec<char> = source.chars().collect();
    let mut budget = 600usize;
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

/// The shortest sequence, and the shortest texts in it, that still fail like the original.
fn shrink_steps(steps: Vec<Step>, still_fails: &dyn Fn(&[Step]) -> bool) -> Vec<Step> {
    let mut steps = steps;
    let mut index = 0;
    while index < steps.len() {
        let mut candidate = steps.clone();
        candidate.remove(index);
        if still_fails(&candidate) {
            steps = candidate;
        } else {
            index += 1;
        }
    }
    for position in 0..steps.len() {
        let Some(text) = steps[position].text().map(str::to_string) else {
            continue;
        };
        let shrunk = shrink(&text, &|candidate| {
            let mut trial = steps.clone();
            trial[position] = steps[position].with_text(candidate);
            still_fails(&trial)
        });
        steps[position] = steps[position].with_text(&shrunk);
    }
    steps
}

struct Found {
    count: usize,
    message: String,
    pair: usize,
    steps: Vec<Step>,
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
    let rounds = env_number("DARTSCOPE_MUTATION_ROUNDS", 25);
    let salt = env_number("DARTSCOPE_MUTATION_SEED", 0) as u64;
    panic::set_hook(Box::new(record_panic));
    let mut runs = 0usize;
    let mut found: BTreeMap<String, Found> = BTreeMap::new();
    for (pair, initial) in PAIRS.iter().enumerate() {
        let mut rng = Rng(
            0x9E37_79B9_7F4A_7C15
                ^ ((pair as u64 + 1) * 0x1000_0000_01B3)
                ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93),
        );
        for _ in 0..rounds {
            let steps = generate(&mut rng, *initial);
            runs += 1;
            let Some(failure) = run(*initial, &steps) else {
                continue;
            };
            let message = match &failure {
                Failure::Panic(_, message) => message.clone(),
                Failure::Divergence(number, _) => format!("at step {number}"),
            };
            let entry = found.entry(failure.key()).or_insert_with(|| Found {
                count: 0,
                message,
                pair,
                steps: steps.clone(),
            });
            entry.count += 1;
            if steps.len() < entry.steps.len() {
                entry.pair = pair;
                entry.steps = steps;
            }
        }
    }
    for (key, entry) in &mut found {
        let initial = PAIRS[entry.pair];
        let steps = std::mem::take(&mut entry.steps);
        entry.steps = shrink_steps(steps, &|candidate| {
            run(initial, candidate).is_some_and(|failure| failure.key() == *key)
        });
    }
    drop(panic::take_hook());

    let report: Vec<String> = found
        .iter()
        .map(|(key, entry)| {
            format!(
                "{key} x{}: {:.60} -- pair {}: {}",
                entry.count,
                entry.message.replace('\n', " "),
                entry.pair,
                entry
                    .steps
                    .iter()
                    .map(Step::describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })
        .collect();
    assert!(report.is_empty(), "\n{}", report.join("\n"));
    assert!(runs >= 80, "only {runs} sequences were run");
}
