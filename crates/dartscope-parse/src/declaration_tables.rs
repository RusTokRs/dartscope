//! Lookup tables over the declarations of one file.
//!
//! The reference passes resolve the same few questions for almost every token: which declaration
//! has this symbol id, which callable is the innermost one around this offset, which member of this
//! type has this name. Each of them used to walk every declaration of the file, which made a file
//! with `n` declarations cost `O(n²)`. The tables answer in constant or logarithmic time and give
//! exactly the declaration the walk returned first, including when spans overlap or ids repeat.

use std::collections::{HashMap, HashSet};

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis};

use crate::interval_index::StabbingIndex;
use crate::member_reference_syntax::declaration_span;

pub(crate) struct DeclarationTables<'a> {
    /// The first declaration with each symbol id.
    by_symbol_id: HashMap<&'a str, usize>,
    /// The first declaration of a kind that can own members with each symbol id.
    owners: HashMap<&'a str, usize>,
    /// Owner symbol id, then member name: the first direct member of that name.
    direct_members: HashMap<&'a str, HashMap<&'a str, usize>>,
    /// Parent symbol id: the names of the instance members it declares.
    instance_members: HashMap<&'a str, HashSet<&'a str>>,
    /// Parent symbol id, then name: the local variables declared there, in source order.
    locals: HashMap<&'a str, HashMap<&'a str, Vec<usize>>>,
    /// Start offsets of all local variable declarations, in increasing order.
    local_starts: Vec<usize>,
    /// Callables that can bind parameters, by their declaration span.
    parameter_callables: StabbingIndex<usize>,
    /// Callables owned by a type, by their declaration span (or their name span without one).
    member_callables: StabbingIndex<usize>,
    /// The same, with top-level and local functions included.
    member_callables_and_functions: StabbingIndex<usize>,
    declarations: &'a [DartDeclaration],
}

impl<'a> DeclarationTables<'a> {
    pub(crate) fn new(analysis: &'a DartFileAnalysis) -> Self {
        let declarations = analysis.declarations.as_slice();
        let mut tables = Self {
            by_symbol_id: HashMap::new(),
            owners: HashMap::new(),
            direct_members: HashMap::new(),
            instance_members: HashMap::new(),
            locals: HashMap::new(),
            local_starts: Vec::new(),
            parameter_callables: StabbingIndex::new(Vec::new()),
            member_callables: StabbingIndex::new(Vec::new()),
            member_callables_and_functions: StabbingIndex::new(Vec::new()),
            declarations,
        };
        let mut parameter_callables = Vec::new();
        let mut member_callables = Vec::new();
        let mut member_callables_and_functions = Vec::new();
        for (index, declaration) in declarations.iter().enumerate() {
            if let Some(symbol_id) = declaration.symbol_id.as_deref() {
                tables.by_symbol_id.entry(symbol_id).or_insert(index);
                if is_member_owner_kind(declaration.kind) {
                    tables.owners.entry(symbol_id).or_insert(index);
                }
                if supports_parameters(declaration.kind)
                    && let Some(span) = declaration.declaration_span.as_ref()
                {
                    parameter_callables.push((
                        span.byte_start,
                        span.byte_end,
                        span.byte_end.saturating_sub(span.byte_start),
                        index,
                    ));
                }
            }
            if let Some(parent) = declaration.parent_symbol_id.as_deref() {
                let name = declaration.name.as_str();
                if is_direct_member_kind(declaration.kind) {
                    tables
                        .direct_members
                        .entry(parent)
                        .or_default()
                        .entry(name)
                        .or_insert(index);
                }
                if is_instance_member_kind(declaration.kind) {
                    tables
                        .instance_members
                        .entry(parent)
                        .or_default()
                        .insert(name);
                }
                if declaration.kind == DartDeclarationKind::LocalVariable {
                    tables
                        .locals
                        .entry(parent)
                        .or_default()
                        .entry(name)
                        .or_default()
                        .push(index);
                }
                if is_callable_kind(declaration.kind) {
                    let span = declaration_span(declaration);
                    let item = (
                        span.byte_start,
                        span.byte_end,
                        span.byte_end.saturating_sub(span.byte_start),
                        index,
                    );
                    member_callables_and_functions.push(item);
                    if declaration.kind != DartDeclarationKind::Function {
                        member_callables.push(item);
                    }
                }
            }
            if declaration.kind == DartDeclarationKind::LocalVariable
                && let Some(span) = declaration.declaration_span.as_ref()
            {
                tables.local_starts.push(span.byte_start);
            }
        }
        tables.local_starts.sort_unstable();
        tables.parameter_callables = StabbingIndex::new(parameter_callables);
        tables.member_callables = StabbingIndex::new(member_callables);
        tables.member_callables_and_functions = StabbingIndex::new(member_callables_and_functions);
        tables
    }

    /// The first declaration with `symbol_id`.
    pub(crate) fn by_symbol_id(&self, symbol_id: &str) -> Option<&'a DartDeclaration> {
        self.by_symbol_id
            .get(symbol_id)
            .map(|&index| &self.declarations[index])
    }

    /// The first declaration with `symbol_id` that is a class, mixin, enum, extension or extension
    /// type.
    pub(crate) fn owner_by_symbol_id(&self, symbol_id: &str) -> Option<&'a DartDeclaration> {
        self.owners
            .get(symbol_id)
            .map(|&index| &self.declarations[index])
    }

    /// The first method, field, getter or setter of the type `owner_symbol_id` called `name`.
    pub(crate) fn direct_member(
        &self,
        owner_symbol_id: &str,
        name: &str,
    ) -> Option<&'a DartDeclaration> {
        self.direct_members
            .get(owner_symbol_id)?
            .get(name)
            .map(|&index| &self.declarations[index])
    }

    /// Whether the declaration `parent_symbol_id` has a method, field, getter, setter or operator
    /// called `name`.
    pub(crate) fn declares_instance_member(&self, parent_symbol_id: &str, name: &str) -> bool {
        self.instance_members
            .get(parent_symbol_id)
            .is_some_and(|names| names.contains(name))
    }

    /// The local variables called `name` that are declared directly in `parent_symbol_id`.
    pub(crate) fn locals_named(
        &self,
        parent_symbol_id: &str,
        name: &str,
    ) -> impl Iterator<Item = &'a DartDeclaration> + '_ {
        self.locals
            .get(parent_symbol_id)
            .and_then(|by_name| by_name.get(name))
            .into_iter()
            .flatten()
            .map(|&index| &self.declarations[index])
    }

    /// Whether some local variable declaration starts in `[start, end)`.
    pub(crate) fn has_local_declaration_starting_in(&self, start: usize, end: usize) -> bool {
        self.local_starts
            .get(self.local_starts.partition_point(|&at| at < start))
            .is_some_and(|&at| at < end)
    }

    /// The symbol id of the smallest function, method, constructor, accessor or operator whose
    /// declaration span contains `offset`.
    pub(crate) fn innermost_callable_symbol(&self, offset: usize) -> Option<&'a str> {
        let stab = self.parameter_callables.best_at(offset)?;
        self.declarations[stab.id].symbol_id.as_deref()
    }

    /// The smallest method, constructor, accessor or operator that belongs to a type and contains
    /// `offset`.
    pub(crate) fn member_callable_at(&self, offset: usize) -> Option<&'a DartDeclaration> {
        let stab = self.member_callables.best_at(offset)?;
        Some(&self.declarations[stab.id])
    }

    /// Like `member_callable_at`, but a function that belongs to a type counts as well.
    pub(crate) fn member_callable_or_function_at(
        &self,
        offset: usize,
    ) -> Option<&'a DartDeclaration> {
        let stab = self.member_callables_and_functions.best_at(offset)?;
        Some(&self.declarations[stab.id])
    }
}

pub(crate) fn supports_parameters(kind: DartDeclarationKind) -> bool {
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

fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
            | DartDeclarationKind::Function
    )
}

pub(crate) fn is_member_owner_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

fn is_direct_member_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Field
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
    )
}

fn is_instance_member_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Method
            | DartDeclarationKind::Field
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

#[cfg(test)]
mod tests;
