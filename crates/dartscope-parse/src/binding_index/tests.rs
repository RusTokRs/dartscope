use std::cmp::Reverse;

use dartscope_core::{DartFileInput, DartLexicalBinding, DartLexicalBindingKind};

use super::{BindingIndex, declarator_segment_start};
use crate::identifiers::{is_identifier_continue, is_identifier_start};
use crate::lexical::mask_non_code;
use crate::source_structure::SourceStructure;

/// The scans that the index replaces, kept as the specification.
mod linear {
    use super::*;

    pub(super) fn statement_start(source: &str, before: usize) -> usize {
        let bytes = source.as_bytes();
        let mut at = before.min(bytes.len());
        while at > 0 {
            at -= 1;
            if matches!(bytes[at], b';' | b'{' | b'}') {
                return at + 1;
            }
        }
        0
    }

    pub(super) fn select_visible<'a>(
        bindings: &'a [DartLexicalBinding],
        name: &str,
        at: usize,
    ) -> Option<&'a DartLexicalBinding> {
        let mut best = None;
        let mut best_rank = None;
        let mut ambiguous = false;
        for binding in bindings.iter().filter(|binding| {
            binding.name == name
                && binding.scope_span.byte_start <= at
                && at < binding.scope_span.byte_end
        }) {
            let rank = (
                binding
                    .scope_span
                    .byte_end
                    .saturating_sub(binding.scope_span.byte_start),
                Reverse(binding.declaration_span.byte_start),
                binding.scope_span.byte_start,
                binding.scope_span.byte_end,
            );
            match best_rank {
                None => {
                    best = Some(binding);
                    best_rank = Some(rank);
                    ambiguous = false;
                }
                Some(current) if rank < current => {
                    best = Some(binding);
                    best_rank = Some(rank);
                    ambiguous = false;
                }
                Some(current) if rank == current => ambiguous = true,
                Some(_) => {}
            }
        }
        if ambiguous { None } else { best }
    }

    pub(super) fn is_visible(bindings: &[DartLexicalBinding], name: &str, at: usize) -> bool {
        bindings.iter().any(|binding| {
            binding.name == name
                && binding.scope_span.byte_start <= at
                && at < binding.scope_span.byte_end
        })
    }

    pub(super) fn is_declaration(
        bindings: &[DartLexicalBinding],
        start: usize,
        end: usize,
    ) -> bool {
        bindings.iter().any(|binding| {
            binding.declaration_span.byte_start <= start && end <= binding.declaration_span.byte_end
        })
    }

    pub(super) fn is_deferred_local_initializer(
        source: &str,
        bindings: &[DartLexicalBinding],
        name: &str,
        at: usize,
    ) -> bool {
        bindings.iter().any(|binding| {
            binding.kind == DartLexicalBindingKind::LocalVariable
                && binding.name == name
                && statement_start(source, binding.declaration_span.byte_start) <= at
                && at < binding.scope_span.byte_start
        })
    }

    pub(super) fn is_local_declaration_prefix(
        source: &str,
        bindings: &[DartLexicalBinding],
        at: usize,
    ) -> bool {
        bindings.iter().any(|binding| {
            if binding.kind != DartLexicalBindingKind::LocalVariable
                || at >= binding.declaration_span.byte_start
            {
                return false;
            }
            let statement_start = statement_start(source, binding.declaration_span.byte_start);
            let segment_start = declarator_segment_start(
                source,
                statement_start,
                binding.declaration_span.byte_start,
            );
            segment_start <= at
        })
    }
}

const SOURCES: &[&str] = &[
    "class A {
  int field = 0;
  void run(int seed, {int other = 1}) {
    var a = seed + field, b = a * 2;
    final c = <int>[a, b].map((v) => v + a).toList();
    for (var i = 0; i < b; i++) { a += i; }
    for (final item in c) { print(item); }
    try { a = 1; } catch (e, st) { print(e); }
    final f = (int x, int y) { return x + y + a; };
    var g = g2 + 1; var g2 = 3;
    int h = h + 1;
    print(a + b + other);
  }
  void other() { var a = 1; { var a = 2; print(a); } print(a); }
}
",
    "void top(int p) { var p2 = p; if (p2 < 3) { var p3 = p2; print(p3); } var p2 = 1; print(p2); }
",
];

fn token_positions(masked: &str) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if !is_identifier_start(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while bytes
            .get(at)
            .is_some_and(|byte| is_identifier_continue(*byte))
        {
            at += 1;
        }
        tokens.push((start, at));
    }
    tokens
}

#[test]
fn queries_agree_with_scans_over_every_token() {
    let mut checked = 0usize;
    for source in SOURCES {
        let analysis = crate::analyze_file_with_references(DartFileInput::new(
            "lib/a.dart",
            (*source).to_string(),
        ));
        assert!(analysis.bindings.len() >= 2, "{source}");
        let masked = mask_non_code(source).code;
        let structure = SourceStructure::new(&masked);
        let index = BindingIndex::new(&masked, &structure, &analysis.bindings);
        for (start, end) in token_positions(&masked) {
            let name = &masked[start..end];
            checked += 1;
            assert_eq!(
                index.select_visible(name, start).map(std::ptr::from_ref),
                linear::select_visible(&analysis.bindings, name, start).map(std::ptr::from_ref),
                "select_visible({name}, {start})"
            );
            assert_eq!(
                index.is_visible(name, start),
                linear::is_visible(&analysis.bindings, name, start),
                "is_visible({name}, {start})"
            );
            assert_eq!(
                index.is_declaration(start, end),
                linear::is_declaration(&analysis.bindings, start, end),
                "is_declaration({name}, {start})"
            );
            assert_eq!(
                index.is_deferred_local_initializer(name, start),
                linear::is_deferred_local_initializer(&masked, &analysis.bindings, name, start),
                "is_deferred_local_initializer({name}, {start})"
            );
            assert_eq!(
                index.is_local_declaration_prefix(start),
                linear::is_local_declaration_prefix(&masked, &analysis.bindings, start),
                "is_local_declaration_prefix({name}, {start})"
            );
        }
        // Positions that are not the start of a token, and names that no binding has.
        for at in 0..=masked.len() + 1 {
            assert_eq!(
                index.select_visible("a", at).map(std::ptr::from_ref),
                linear::select_visible(&analysis.bindings, "a", at).map(std::ptr::from_ref),
                "select_visible(a, {at})"
            );
            assert!(!index.is_visible("missing", at));
            assert_eq!(
                index.is_local_declaration_prefix(at),
                linear::is_local_declaration_prefix(&masked, &analysis.bindings, at),
                "is_local_declaration_prefix({at})"
            );
        }
        for binding in &analysis.bindings {
            let owned: Vec<_> = index
                .owned_by(&binding.enclosing_symbol_id, &binding.name)
                .map(std::ptr::from_ref)
                .collect();
            let expected: Vec<_> = analysis
                .bindings
                .iter()
                .filter(|other| {
                    other.enclosing_symbol_id == binding.enclosing_symbol_id
                        && other.name == binding.name
                })
                .map(std::ptr::from_ref)
                .collect();
            assert_eq!(owned, expected);
        }
    }
    assert!(checked > 60, "only {checked} tokens were compared");
}

#[test]
fn equal_ranks_are_ambiguous() {
    // Two bindings of one name with the same scope and declaration offset compete; neither wins.
    let source = "void f(int a) { print(a); }";
    let analysis =
        crate::analyze_file_with_references(DartFileInput::new("lib/a.dart", source.to_string()));
    let first = analysis
        .bindings
        .iter()
        .find(|binding| binding.name == "a")
        .expect("a binding of a")
        .clone();
    let doubled = vec![first.clone(), first.clone()];
    let masked = mask_non_code(source).code;
    let structure = SourceStructure::new(&masked);
    let index = BindingIndex::new(&masked, &structure, &doubled);
    let at = first.scope_span.byte_start;
    assert!(index.select_visible("a", at).is_none());
    assert!(index.is_visible("a", at));
    assert!(linear::select_visible(&doubled, "a", at).is_none());
}
