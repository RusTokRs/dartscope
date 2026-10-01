//! Which classes are widgets, by following `extends` through the classes of a project.
//!
//! A class is a widget when its superclass chain ends at one of the Flutter base classes. The chain
//! is followed by simple class name through the declarations that were given, so a base class
//! declared in another file of the project is found, and so is a base class of a base class. Only
//! the superclass matters: a mixin (`with M`, `mixin M on StatelessWidget`) cannot change what a
//! class extends, and a class that applies a mixin constrained to a widget has to extend a widget
//! itself.

use std::collections::HashMap;

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartFileAnalysis};

/// The Flutter base classes the widget convention recognizes, by simple name.
pub(crate) fn flutter_base(name: &str) -> Option<&'static str> {
    const BASES: [&str; 6] = [
        "Widget",
        "StatelessWidget",
        "StatefulWidget",
        "InheritedWidget",
        "State",
        "ConsumerWidget",
    ];
    let simple = simple_name(name);
    BASES.into_iter().find(|base| *base == simple)
}

/// `alias.Name` is `Name`: an import prefix does not change which class is meant.
fn simple_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// How a widget class reaches its Flutter base class.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ReachedBase<'a> {
    /// The Flutter base class at the end of the chain.
    pub(crate) base: &'static str,
    /// The superclass named in `extends` when it is a project class rather than the base itself.
    pub(crate) via: Option<&'a str>,
}

/// The widget classes of a set of files, keyed by file path and declaration start.
pub(crate) struct WidgetClasses<'a> {
    reached: HashMap<&'a str, HashMap<usize, ReachedBase<'a>>>,
}

/// What a class says about its superclass.
#[derive(Clone, Copy)]
enum Link {
    /// `extends` names a Flutter base class.
    Base(&'static str),
    /// `extends` names exactly one other class of the files.
    Class(usize),
    /// No superclass, a superclass outside the files, or a name that is declared more than once.
    Open,
}

struct ClassEntry<'a> {
    path: &'a str,
    declaration: &'a DartDeclaration,
}

impl<'a> WidgetClasses<'a> {
    pub(crate) fn new(files: &'a [DartFileAnalysis]) -> Self {
        let classes: Vec<ClassEntry<'a>> = files
            .iter()
            .flat_map(|file| {
                file.declarations
                    .iter()
                    .filter(|declaration| declaration.kind == DartDeclarationKind::Class)
                    .map(move |declaration| ClassEntry {
                        path: file.path.as_str(),
                        declaration,
                    })
            })
            .collect();
        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, class) in classes.iter().enumerate() {
            by_name
                .entry(class.declaration.name.as_str())
                .or_default()
                .push(index);
        }

        let links: Vec<Link> = classes
            .iter()
            .map(|class| link_of(class, &classes, &by_name))
            .collect();
        let outcomes = follow_links(&links);

        let mut reached: HashMap<&'a str, HashMap<usize, ReachedBase<'a>>> = HashMap::new();
        for (class, outcome) in classes.iter().zip(&outcomes) {
            let Some(base) = *outcome else {
                continue;
            };
            let via = class
                .declaration
                .extends
                .as_deref()
                .filter(|extends| flutter_base(extends).is_none());
            reached
                .entry(class.path)
                .or_default()
                .insert(class.declaration.span.byte_start, ReachedBase { base, via });
        }
        Self { reached }
    }

    /// The Flutter base class `declaration` of the file at `path` extends, directly or not.
    pub(crate) fn reached(
        &self,
        path: &str,
        declaration: &DartDeclaration,
    ) -> Option<ReachedBase<'a>> {
        self.reached
            .get(path)?
            .get(&declaration.span.byte_start)
            .copied()
    }
}

fn link_of(class: &ClassEntry<'_>, classes: &[ClassEntry<'_>], by_name: &HashMap<&str, Vec<usize>>) -> Link {
    let Some(extends) = class.declaration.extends.as_deref() else {
        return Link::Open;
    };
    if let Some(base) = flutter_base(extends) {
        return Link::Base(base);
    }
    let name = simple_name(extends);
    let Some(candidates) = by_name.get(name) else {
        return Link::Open;
    };
    // A private name belongs to its own library, so only the file that declares it can mean it.
    let private = name.starts_with('_');
    let mut visible = candidates
        .iter()
        .copied()
        .filter(|candidate| !private || classes[*candidate].path == class.path);
    match (visible.next(), visible.next()) {
        // Without the import graph, two classes of one name cannot be told apart, so none is
        // guessed.
        (Some(only), None) => Link::Class(only),
        _ => Link::Open,
    }
}

/// Follows every class along its chain once: a chain that reaches a base class gives the base to
/// all classes on it, and a chain that ends elsewhere or loops gives them none. Iterative, so a
/// chain of any length is safe.
fn follow_links(links: &[Link]) -> Vec<Option<&'static str>> {
    let mut outcomes: Vec<Option<Option<&'static str>>> = vec![None; links.len()];
    let mut on_path = vec![false; links.len()];
    for start in 0..links.len() {
        if outcomes[start].is_some() {
            continue;
        }
        let mut path = Vec::new();
        let mut current = start;
        let outcome = loop {
            if let Some(known) = outcomes[current] {
                break known;
            }
            if on_path[current] {
                break None;
            }
            on_path[current] = true;
            path.push(current);
            match links[current] {
                Link::Base(base) => break Some(base),
                Link::Class(next) => current = next,
                Link::Open => break None,
            }
        };
        for index in path {
            on_path[index] = false;
            outcomes[index] = Some(outcome);
        }
    }
    outcomes.into_iter().map(Option::unwrap_or_default).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_reach_a_base_and_loops_and_gaps_do_not() {
        // 0 -> 1 -> 2 -> StatelessWidget; 3 <-> 4; 5 -> 3; 6 has no superclass; 7 -> 6.
        let links = [
            Link::Class(1),
            Link::Class(2),
            Link::Base("StatelessWidget"),
            Link::Class(4),
            Link::Class(3),
            Link::Class(3),
            Link::Open,
            Link::Class(6),
        ];

        assert_eq!(
            follow_links(&links),
            [
                Some("StatelessWidget"),
                Some("StatelessWidget"),
                Some("StatelessWidget"),
                None,
                None,
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn a_very_long_chain_does_not_recurse() {
        let length = 200_000;
        let mut links: Vec<Link> = (1..length).map(Link::Class).collect();
        links.push(Link::Base("State"));

        let outcomes = follow_links(&links);

        assert_eq!(outcomes.len(), length);
        assert!(outcomes.iter().all(|outcome| *outcome == Some("State")));
    }

    #[test]
    fn base_names_ignore_an_import_prefix() {
        assert_eq!(flutter_base("material.StatelessWidget"), Some("StatelessWidget"));
        assert_eq!(flutter_base("StatefulWidgetX"), None);
        assert_eq!(flutter_base("Object"), None);
    }
}
