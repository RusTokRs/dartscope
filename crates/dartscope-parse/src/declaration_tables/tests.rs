use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartFileInput};

use super::{DeclarationTables, is_member_owner_kind, supports_parameters};
use crate::member_reference_syntax::declaration_span;

/// The walks that the tables replace, kept as the specification.
mod linear {
    use super::*;

    pub(super) fn by_symbol_id<'a>(
        analysis: &'a DartFileAnalysis,
        symbol_id: &str,
    ) -> Option<&'a DartDeclaration> {
        analysis
            .declarations
            .iter()
            .find(|declaration| declaration.symbol_id.as_deref() == Some(symbol_id))
    }

    pub(super) fn owner_by_symbol_id<'a>(
        analysis: &'a DartFileAnalysis,
        symbol_id: &str,
    ) -> Option<&'a DartDeclaration> {
        analysis.declarations.iter().find(|declaration| {
            declaration.symbol_id.as_deref() == Some(symbol_id)
                && is_member_owner_kind(declaration.kind)
        })
    }

    pub(super) fn direct_member<'a>(
        analysis: &'a DartFileAnalysis,
        owner_symbol_id: &str,
        name: &str,
    ) -> Option<&'a DartDeclaration> {
        analysis.declarations.iter().find(|declaration| {
            declaration.name == name
                && declaration.parent_symbol_id.as_deref() == Some(owner_symbol_id)
                && matches!(
                    declaration.kind,
                    DartDeclarationKind::Method
                        | DartDeclarationKind::Field
                        | DartDeclarationKind::Getter
                        | DartDeclarationKind::Setter
                )
        })
    }

    pub(super) fn declares_instance_member(
        analysis: &DartFileAnalysis,
        parent_symbol_id: &str,
        name: &str,
    ) -> bool {
        analysis.declarations.iter().any(|declaration| {
            declaration.parent_symbol_id.as_deref() == Some(parent_symbol_id)
                && declaration.name == name
                && matches!(
                    declaration.kind,
                    DartDeclarationKind::Method
                        | DartDeclarationKind::Field
                        | DartDeclarationKind::Getter
                        | DartDeclarationKind::Setter
                        | DartDeclarationKind::Operator
                )
        })
    }

    pub(super) fn locals_named<'a>(
        analysis: &'a DartFileAnalysis,
        parent_symbol_id: &str,
        name: &str,
    ) -> Vec<&'a DartDeclaration> {
        analysis
            .declarations
            .iter()
            .filter(|declaration| {
                declaration.kind == DartDeclarationKind::LocalVariable
                    && declaration.name == name
                    && declaration.parent_symbol_id.as_deref() == Some(parent_symbol_id)
            })
            .collect()
    }

    pub(super) fn has_local_declaration_starting_in(
        analysis: &DartFileAnalysis,
        start: usize,
        end: usize,
    ) -> bool {
        analysis.declarations.iter().any(|declaration| {
            declaration.kind == DartDeclarationKind::LocalVariable
                && declaration
                    .declaration_span
                    .as_ref()
                    .is_some_and(|span| start <= span.byte_start && span.byte_start < end)
        })
    }

    pub(super) fn innermost_callable_symbol(
        analysis: &DartFileAnalysis,
        offset: usize,
    ) -> Option<String> {
        analysis
            .declarations
            .iter()
            .filter(|declaration| supports_parameters(declaration.kind))
            .filter_map(|declaration| {
                let span = declaration.declaration_span.as_ref()?;
                (span.byte_start <= offset && offset < span.byte_end).then_some((
                    span.byte_end.saturating_sub(span.byte_start),
                    declaration.symbol_id.as_ref()?,
                ))
            })
            .min_by_key(|(length, _)| *length)
            .map(|(_, symbol_id)| symbol_id.clone())
    }

    pub(super) fn member_callable_at(
        analysis: &DartFileAnalysis,
        offset: usize,
        with_functions: bool,
    ) -> Option<&DartDeclaration> {
        analysis
            .declarations
            .iter()
            .filter(|declaration| {
                (matches!(
                    declaration.kind,
                    DartDeclarationKind::Method
                        | DartDeclarationKind::Constructor
                        | DartDeclarationKind::Getter
                        | DartDeclarationKind::Setter
                        | DartDeclarationKind::Operator
                ) || (with_functions && declaration.kind == DartDeclarationKind::Function))
                    && declaration.parent_symbol_id.is_some()
            })
            .filter(|declaration| {
                let span = declaration_span(declaration);
                span.byte_start <= offset && offset < span.byte_end
            })
            .min_by_key(|declaration| {
                let span = declaration_span(declaration);
                span.byte_end.saturating_sub(span.byte_start)
            })
    }
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        usize::try_from(value % bound as u64).unwrap_or(0)
    }
}

const SOURCES: &[&str] = &[
    "import 'package:flutter/material.dart';

class Counter extends StatefulWidget {
  const Counter({super.key});
  @override
  State<Counter> createState() => _CounterState();
}

class _CounterState extends State<Counter> {
  int _count = 0;
  static const int limit = 3;
  int get double => _count * 2;
  set count(int value) { _count = value; }
  int operator +(int other) => _count + other;
  void bump(int step) {
    var total = _count + step;
    for (var i = 0; i < step; i++) { total += i; }
    setState(() { _count = total; });
  }
  Widget build(BuildContext context) => Text('$_count');
}

enum Color { red(1), green(2); const Color(this.code); final int code; }
mixin Logs on Object { void log(String m) { print(m); } }
extension Doubling on int { int twice() => this * 2; }
int helper(int a) { var b = a; return b; }
",
    "class A { int x = 0; void f() { var x = 1; { var y = x; } } }
class A { void f() {} int f2() => 1; }
",
    "abstract class B<T> { T value; B(this.value); T get v => value; set v(T t) { value = t; } void m<R>(R r) {} }
",
];

fn analyses() -> Vec<DartFileAnalysis> {
    let mut rng = Rng(0xFEED_FACE_CAFE_BEEF);
    let mut result = Vec::new();
    for source in SOURCES {
        let analysis = crate::analyze_file(DartFileInput::new("lib/a.dart", (*source).to_string()));
        assert!(analysis.declarations.len() >= 4, "{source}");
        // The same declarations with spans that overlap in ways real code cannot: the tables must
        // keep the answer of the walk when they are not nested.
        for _ in 0..8 {
            let mut shuffled = analysis.clone();
            for declaration in &mut shuffled.declarations {
                if let Some(span) = declaration.declaration_span.as_mut() {
                    span.byte_start = rng.below(source.len());
                    span.byte_end = span.byte_start + rng.below(source.len() / 2 + 1);
                }
                if rng.below(4) == 0 {
                    declaration.declaration_span = None;
                }
                if rng.below(5) == 0 {
                    declaration.symbol_id = None;
                }
            }
            result.push(shuffled);
        }
        result.push(analysis);
    }
    result
}

#[test]
fn symbol_lookups_agree_with_a_walk() {
    for analysis in analyses() {
        let tables = DeclarationTables::new(&analysis);
        let mut ids: Vec<&str> = analysis
            .declarations
            .iter()
            .filter_map(|declaration| declaration.symbol_id.as_deref())
            .collect();
        ids.push("missing");
        for id in ids {
            assert_eq!(
                tables.by_symbol_id(id).map(std::ptr::from_ref),
                linear::by_symbol_id(&analysis, id).map(std::ptr::from_ref),
                "by_symbol_id({id})"
            );
            assert_eq!(
                tables.owner_by_symbol_id(id).map(std::ptr::from_ref),
                linear::owner_by_symbol_id(&analysis, id).map(std::ptr::from_ref),
                "owner_by_symbol_id({id})"
            );
            let mut names: Vec<&str> = analysis
                .declarations
                .iter()
                .map(|declaration| declaration.name.as_str())
                .collect();
            names.push("missing");
            for name in names {
                assert_eq!(
                    tables.direct_member(id, name).map(std::ptr::from_ref),
                    linear::direct_member(&analysis, id, name).map(std::ptr::from_ref),
                    "direct_member({id}, {name})"
                );
                assert_eq!(
                    tables.declares_instance_member(id, name),
                    linear::declares_instance_member(&analysis, id, name),
                    "declares_instance_member({id}, {name})"
                );
                assert_eq!(
                    tables
                        .locals_named(id, name)
                        .map(std::ptr::from_ref)
                        .collect::<Vec<_>>(),
                    linear::locals_named(&analysis, id, name)
                        .into_iter()
                        .map(std::ptr::from_ref)
                        .collect::<Vec<_>>(),
                    "locals_named({id}, {name})"
                );
            }
        }
    }
}

#[test]
fn position_lookups_agree_with_a_walk() {
    for analysis in analyses() {
        let tables = DeclarationTables::new(&analysis);
        let limit = SOURCES.iter().map(|source| source.len()).max().unwrap_or(0) + 3;
        for offset in 0..limit {
            assert_eq!(
                tables.innermost_callable_symbol(offset).map(str::to_string),
                linear::innermost_callable_symbol(&analysis, offset),
                "innermost_callable_symbol({offset})"
            );
            assert_eq!(
                tables.member_callable_at(offset).map(std::ptr::from_ref),
                linear::member_callable_at(&analysis, offset, false).map(std::ptr::from_ref),
                "member_callable_at({offset})"
            );
            assert_eq!(
                tables
                    .member_callable_or_function_at(offset)
                    .map(std::ptr::from_ref),
                linear::member_callable_at(&analysis, offset, true).map(std::ptr::from_ref),
                "member_callable_or_function_at({offset})"
            );
            for end in [offset, offset + 1, offset + 7, limit] {
                assert_eq!(
                    tables.has_local_declaration_starting_in(offset, end),
                    linear::has_local_declaration_starting_in(&analysis, offset, end),
                    "has_local_declaration_starting_in({offset}, {end})"
                );
            }
        }
    }
}
