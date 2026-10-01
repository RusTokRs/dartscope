//! Lookups over the lexical bindings of one file.
//!
//! The read, write and update passes ask the same questions of every identifier: does it spell a
//! binding's own declaration, does it sit in front of a local variable that is being declared, which
//! binding is the innermost visible one for its name. Asking the whole list of bindings each time
//! made a file cost `O(identifiers × bindings)`; this index is built once per pass and answers each
//! question in logarithmic time with the answer the scan gave.

use std::cmp::Reverse;
use std::collections::HashMap;

use dartscope_core::{DartIdentifierReference, DartLexicalBinding, DartLexicalBindingKind};

use crate::interval_index::{IntervalSet, StabbingIndex};
use crate::source_structure::SourceStructure;

/// Orders the bindings visible at one position: the smallest rank is the innermost binding.
type BindingRank = (usize, Reverse<usize>, usize, usize);

pub(crate) struct BindingIndex<'a> {
    bindings: &'a [DartLexicalBinding],
    /// The spans of the identifiers that declare a binding.
    declarations: IntervalSet,
    /// By name: from the start of the statement of a local variable up to where its scope starts.
    initializer_prefixes: HashMap<&'a str, IntervalSet>,
    /// From the start of the declarator of each local variable up to the variable's name.
    declaration_prefixes: IntervalSet,
    /// By name: the scopes of the bindings of that name.
    visible: HashMap<&'a str, StabbingIndex<BindingRank>>,
    /// By enclosing symbol id, then name: the indexes of the bindings.
    by_owner: HashMap<&'a str, HashMap<&'a str, Vec<usize>>>,
}

impl<'a> BindingIndex<'a> {
    pub(crate) fn new(
        masked_source: &str,
        structure: &SourceStructure,
        bindings: &'a [DartLexicalBinding],
    ) -> Self {
        let declarations = IntervalSet::new(bindings.iter().map(|binding| {
            (
                binding.declaration_span.byte_start,
                binding.declaration_span.byte_end,
            )
        }));
        let mut initializer_prefixes: HashMap<&'a str, Vec<(usize, usize)>> = HashMap::new();
        let mut declaration_prefixes = Vec::new();
        let mut visible: HashMap<&'a str, Vec<(usize, usize, BindingRank, usize)>> = HashMap::new();
        let mut by_owner: HashMap<&'a str, HashMap<&'a str, Vec<usize>>> = HashMap::new();
        for (index, binding) in bindings.iter().enumerate() {
            let name = binding.name.as_str();
            if binding.kind == DartLexicalBindingKind::LocalVariable {
                let declaration_start = binding.declaration_span.byte_start;
                let statement_start = structure.statement_start(declaration_start);
                initializer_prefixes
                    .entry(name)
                    .or_default()
                    .push((statement_start, binding.scope_span.byte_start));
                declaration_prefixes.push((
                    declarator_segment_start(masked_source, statement_start, declaration_start),
                    declaration_start,
                ));
            }
            visible.entry(name).or_default().push((
                binding.scope_span.byte_start,
                binding.scope_span.byte_end,
                binding_rank(binding),
                index,
            ));
            by_owner
                .entry(binding.enclosing_symbol_id.as_str())
                .or_default()
                .entry(name)
                .or_default()
                .push(index);
        }
        Self {
            bindings,
            declarations,
            initializer_prefixes: initializer_prefixes
                .into_iter()
                .map(|(name, intervals)| (name, IntervalSet::new(intervals)))
                .collect(),
            declaration_prefixes: IntervalSet::new(declaration_prefixes),
            visible: visible
                .into_iter()
                .map(|(name, items)| (name, StabbingIndex::new(items)))
                .collect(),
            by_owner,
        }
    }

    /// Whether `[start, end)` lies inside the identifier that declares some binding.
    pub(crate) fn is_declaration(&self, start: usize, end: usize) -> bool {
        self.declarations.covers(start, end)
    }

    /// Whether an identifier `name` at `at` is part of the statement that declares a local
    /// variable of that name before the variable's scope starts (its own initializer).
    pub(crate) fn is_deferred_local_initializer(&self, name: &str, at: usize) -> bool {
        self.initializer_prefixes
            .get(name)
            .is_some_and(|prefixes| prefixes.contains(at))
    }

    /// Whether `at` is in front of the name of a local variable, within its declarator.
    pub(crate) fn is_local_declaration_prefix(&self, at: usize) -> bool {
        self.declaration_prefixes.contains(at)
    }

    /// The innermost binding of `name` whose scope contains `at`, unless two of equal rank compete.
    pub(crate) fn select_visible(&self, name: &str, at: usize) -> Option<&'a DartLexicalBinding> {
        let stab = self.visible.get(name)?.best_at(at)?;
        (!stab.tied).then(|| &self.bindings[stab.id])
    }

    /// Whether some binding of `name` has a scope that contains `at`.
    pub(crate) fn is_visible(&self, name: &str, at: usize) -> bool {
        self.visible
            .get(name)
            .is_some_and(|scopes| scopes.best_at(at).is_some())
    }

    /// The bindings called `name` that belong to the callable `owner_symbol_id`.
    pub(crate) fn owned_by(
        &self,
        owner_symbol_id: &str,
        name: &str,
    ) -> impl Iterator<Item = &'a DartLexicalBinding> + '_ {
        self.by_owner
            .get(owner_symbol_id)
            .and_then(|by_name| by_name.get(name))
            .into_iter()
            .flatten()
            .map(|&index| &self.bindings[index])
    }
}

/// The spans of the references that were already found, for asking whether a token overlaps one.
pub(crate) fn reference_spans(references: &[DartIdentifierReference]) -> IntervalSet {
    IntervalSet::new(
        references
            .iter()
            .map(|reference| (reference.span.byte_start, reference.span.byte_end)),
    )
}

fn binding_rank(binding: &DartLexicalBinding) -> BindingRank {
    (
        binding
            .scope_span
            .byte_end
            .saturating_sub(binding.scope_span.byte_start),
        Reverse(binding.declaration_span.byte_start),
        binding.scope_span.byte_start,
        binding.scope_span.byte_end,
    )
}

/// Where the declarator that ends at `end` begins: just after the last comma that is not nested
/// in brackets or angle brackets, searching back to the start of the statement.
fn declarator_segment_start(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = end.min(bytes.len());
    while at > start {
        at -= 1;
        match bytes[at] {
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                return at + 1;
            }
            b')' => parens += 1,
            b'(' => parens = parens.saturating_sub(1),
            b']' => brackets += 1,
            b'[' => brackets = brackets.saturating_sub(1),
            b'}' => braces += 1,
            b'{' => braces = braces.saturating_sub(1),
            b'>' => angles += 1,
            b'<' => angles = angles.saturating_sub(1),
            _ => {}
        }
    }
    start
}

#[cfg(test)]
mod tests;
