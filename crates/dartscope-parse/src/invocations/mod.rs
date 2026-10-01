//! Parser-independent invocation facts for the conservative backend.

mod arguments;
mod scanner;

use std::collections::HashMap;

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartDiagnostic, DartInvocation, DartInvocationArgument,
    SourceSpan,
};

use self::arguments::invocation_arguments;
use self::scanner::{CallCandidate, CopyBudget, Delimiters, scan_call_candidates};
use crate::source_lines::{line_span_for_byte, span_for_byte_range};

/// The invocation facts of one file.
pub(crate) struct InvocationFacts {
    pub(crate) invocations: Vec<DartInvocation>,
    /// Present when the facts stop before the end of the file because the call targets or the
    /// arguments would copy more source text than the file's budget allows.
    pub(crate) truncated: Option<DartDiagnostic>,
}

pub(crate) fn collect_invocations(
    source: &str,
    masked_source: &str,
    declarations: &[DartDeclaration],
) -> InvocationFacts {
    let lookup = DeclarationLookup::new(masked_source, declarations);
    let delimiters = Delimiters::new(masked_source);
    let scan = scan_call_candidates(
        masked_source,
        &delimiters,
        &mut CopyBudget::for_source(source.len()),
    );
    let mut cut_at = scan.stopped_at.map(|at| line_span_for_byte(source, at));
    let mut argument_budget = CopyBudget::for_source(source.len());
    let mut invocations = Vec::new();
    for candidate in scan.candidates {
        if lookup.is_header_call(&candidate) {
            continue;
        }
        let invocation =
            invocation_from_candidate(source, masked_source, &lookup, &delimiters, candidate);
        if !argument_budget.spend(argument_text_len(&invocation)) {
            cut_at = Some(invocation.source_line_span.clone());
            break;
        }
        invocations.push(invocation);
    }
    invocations.sort_by(|left, right| {
        (left.span.byte_start, left.span.byte_end, &left.target).cmp(&(
            right.span.byte_start,
            right.span.byte_end,
            &right.target,
        ))
    });
    invocations.dedup_by(|left, right| {
        left.span.byte_start == right.span.byte_start
            && left.span.byte_end == right.span.byte_end
            && left.target == right.target
    });
    InvocationFacts {
        invocations,
        truncated: cut_at.map(|span| {
            DartDiagnostic::warning(
                "invocation_facts_truncated",
                "invocation facts stop here: the call targets and arguments of this file would copy more source text than the per-file budget of 32 times its size plus 1 MiB allows",
                Some(span),
            )
        }),
    }
}

/// Bytes of source text that the arguments of an invocation hold: the argument expressions and
/// the keys and values of their map entries.
fn argument_text_len(invocation: &DartInvocation) -> usize {
    invocation
        .arguments
        .iter()
        .map(|argument| {
            argument.expression.len()
                + argument
                    .map_entries
                    .iter()
                    .map(|entry| entry.key.len() + entry.value.len())
                    .sum::<usize>()
        })
        .sum()
}

/// Lookup structures over the declarations of one file.
///
/// Every call candidate has to be classified against the declarations: is it the name inside a
/// declaration's own header, and which callable contains it? Scanning all declarations for each
/// candidate is quadratic in the size of the file, so the answers come from structures built once.
struct DeclarationLookup<'a> {
    /// Callable declarations ordered by span start, outer spans before the spans nested in them.
    callables: Vec<CallableSpan<'a>>,
    /// `(start, header_end)` of every declaration, by declared name, ordered by start.
    ///
    /// The headers of declarations that share a name do not overlap in well-formed source (a class
    /// header ends at its `{`, before the constructors that carry the same name), so only the
    /// header that starts last before a position can contain it.
    headers: HashMap<&'a str, Vec<(usize, usize)>>,
}

struct CallableSpan<'a> {
    start: usize,
    end: usize,
    symbol_id: Option<&'a str>,
    /// Index of the nearest callable whose span encloses this one.
    parent: Option<usize>,
}

impl<'a> DeclarationLookup<'a> {
    fn new(masked_source: &str, declarations: &'a [DartDeclaration]) -> Self {
        let mut callables = Vec::new();
        let mut headers: HashMap<&'a str, Vec<(usize, usize)>> = HashMap::new();
        for declaration in declarations {
            let Some(span) = declaration.declaration_span.as_ref() else {
                continue;
            };
            headers
                .entry(declaration.name.as_str())
                .or_default()
                .push((span.byte_start, declaration_header_end(masked_source, span)));
            if is_callable_kind(declaration.kind) {
                callables.push(CallableSpan {
                    start: span.byte_start,
                    end: span.byte_end,
                    symbol_id: declaration.symbol_id.as_deref(),
                    parent: None,
                });
            }
        }
        for ranges in headers.values_mut() {
            ranges.sort_unstable();
        }
        callables.sort_by_key(|callable| (callable.start, std::cmp::Reverse(callable.end)));

        let mut parents = Vec::with_capacity(callables.len());
        let mut open: Vec<usize> = Vec::new();
        for (index, callable) in callables.iter().enumerate() {
            while open
                .last()
                .is_some_and(|&outer| callables[outer].end <= callable.start)
            {
                open.pop();
            }
            parents.push(open.last().copied());
            open.push(index);
        }
        for (callable, parent) in callables.iter_mut().zip(parents) {
            callable.parent = parent;
        }
        Self { callables, headers }
    }

    /// The symbol id of the innermost callable whose span contains `at`.
    fn enclosing_symbol_id(&self, at: usize) -> Option<String> {
        // The last callable that starts at or before `at` is either the innermost container or
        // nested in it, so the container is found by walking outwards from there.
        let mut index = self
            .callables
            .partition_point(|callable| callable.start <= at)
            .checked_sub(1)?;
        loop {
            let callable = &self.callables[index];
            if at < callable.end {
                return callable.symbol_id.map(str::to_owned);
            }
            index = callable.parent?;
        }
    }

    /// Whether the candidate is the declared name inside the header of a declaration with that name.
    fn is_header_call(&self, candidate: &CallCandidate) -> bool {
        let Some(ranges) = self.headers.get(candidate.target.as_str()) else {
            return false;
        };
        ranges
            .partition_point(|&(start, _)| start <= candidate.start)
            .checked_sub(1)
            .is_some_and(|index| candidate.start < ranges[index].1)
    }
}

fn invocation_from_candidate(
    source: &str,
    masked_source: &str,
    lookup: &DeclarationLookup<'_>,
    delimiters: &Delimiters,
    candidate: CallCandidate,
) -> DartInvocation {
    let arguments: Vec<DartInvocationArgument> = invocation_arguments(
        source,
        masked_source,
        candidate.open + 1,
        candidate.close,
        delimiters,
    );
    DartInvocation {
        target: candidate.target,
        arguments,
        result_members: candidate.result_members,
        enclosing_symbol_id: lookup.enclosing_symbol_id(candidate.start),
        span: span_for_byte_range(source, candidate.start, candidate.end),
        source_line_span: line_span_for_byte(source, candidate.start),
    }
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut index = span.byte_start;
    while index < span.byte_end.min(bytes.len()) {
        match bytes[index] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 => return index,
            b'=' if parens == 0 && brackets == 0 && bytes.get(index + 1) == Some(&b'>') => {
                return index;
            }
            _ => {}
        }
        index += 1;
    }
    span.byte_end
}

fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration_inventory::collect_declaration_inventory;
    use crate::lexical::mask_non_code;

    const SOURCE: &str = "\
import 'package:a/a.dart';

int helper(int a) => a + 1;

class Counter {
  final int start = seed();
  int value = compute(1);

  Counter(this.value) : assert(value > 0);
  Counter.named(int v) : value = clamp(v);

  int next() {
    final step = helper(value);
    return wrap(step);
  }

  int get double => value * 2;

  final late = tail();

  void reset() => value = fallback();
}

class Plain {
  Plain();
  factory Plain.of(int x) => Plain();
  void run() {
    helper(3);
    Plain.of(4);
  }
}

void main() {
  final c = Counter(1);
  c.next();
  print(helper(2));
}
";

    fn linear_enclosing_symbol_id(at: usize, declarations: &[DartDeclaration]) -> Option<String> {
        declarations
            .iter()
            .filter(|declaration| is_callable_kind(declaration.kind))
            .filter_map(|declaration| {
                let span = declaration.declaration_span.as_ref()?;
                (span.byte_start <= at && at < span.byte_end).then_some((
                    span.byte_end.saturating_sub(span.byte_start),
                    declaration.symbol_id.as_ref(),
                ))
            })
            .min_by_key(|(length, _)| *length)
            .and_then(|(_, symbol_id)| symbol_id.cloned())
    }

    fn linear_is_header_call(
        candidate: &CallCandidate,
        masked_source: &str,
        declarations: &[DartDeclaration],
    ) -> bool {
        declarations.iter().any(|declaration| {
            let Some(span) = declaration.declaration_span.as_ref() else {
                return false;
            };
            if candidate.start < span.byte_start || candidate.start >= span.byte_end {
                return false;
            }
            let header_end = declaration_header_end(masked_source, span);
            if candidate.start >= header_end {
                return false;
            }
            candidate.target == declaration.name
        })
    }

    #[test]
    fn indexed_lookups_agree_with_a_scan_over_all_declarations() {
        let masked = mask_non_code(SOURCE).code;
        let (declarations, _) = collect_declaration_inventory("lib/a.dart", SOURCE, &masked);
        let lookup = DeclarationLookup::new(&masked, &declarations);

        for at in 0..=SOURCE.len() {
            assert_eq!(
                lookup.enclosing_symbol_id(at),
                linear_enclosing_symbol_id(at, &declarations),
                "innermost callable at byte {at}"
            );
        }

        let candidates = scan_call_candidates(
            &masked,
            &Delimiters::new(&masked),
            &mut CopyBudget::for_source(masked.len()),
        )
        .candidates;
        let header_calls = candidates
            .iter()
            .filter(|candidate| lookup.is_header_call(candidate))
            .count();
        assert!(candidates.len() > 12, "{} candidates", candidates.len());
        assert!(header_calls >= 5, "{header_calls} header calls");
        for candidate in &candidates {
            assert_eq!(
                lookup.is_header_call(candidate),
                linear_is_header_call(candidate, &masked, &declarations),
                "header call `{}` at byte {}",
                candidate.target,
                candidate.start
            );
        }
    }

    #[test]
    fn calls_outside_every_callable_have_no_enclosing_symbol() {
        let masked = mask_non_code(SOURCE).code;
        let (declarations, _) = collect_declaration_inventory("lib/a.dart", SOURCE, &masked);
        let lookup = DeclarationLookup::new(&masked, &declarations);

        let field_initializer = SOURCE.find("tail()").expect("field initializer call");
        assert_eq!(lookup.enclosing_symbol_id(field_initializer), None);
        let in_next = SOURCE.find("wrap(step)").expect("call in next()");
        let next = declarations
            .iter()
            .find(|declaration| {
                declaration.kind == DartDeclarationKind::Method && declaration.name == "next"
            })
            .expect("method next");
        assert!(next.symbol_id.is_some());
        assert_eq!(lookup.enclosing_symbol_id(in_next), next.symbol_id);
    }
}
