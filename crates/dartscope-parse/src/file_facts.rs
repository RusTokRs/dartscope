//! The lookup structures that the reference passes share for one file.
//!
//! They are built once from the file's declarations and masked text, so each pass can answer its
//! per-token questions without walking the declarations or rescanning the text; see
//! `DeclarationTables` and `SourceStructure` for what they answer.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use dartscope_core::{DartDeclaration, DartFileAnalysis};

use crate::declaration_tables::DeclarationTables;
use crate::source_structure::SourceStructure;

pub(crate) struct FileFacts<'a> {
    pub(crate) tables: DeclarationTables<'a>,
    pub(crate) structure: SourceStructure,
    import_prefixes: HashSet<&'a str>,
    /// Names declared as local functions inside a callable, by the address of the callable.
    local_functions: RefCell<HashMap<usize, HashSet<String>>>,
}

impl<'a> FileFacts<'a> {
    pub(crate) fn new(masked_source: &str, analysis: &'a DartFileAnalysis) -> Self {
        Self {
            tables: DeclarationTables::new(analysis),
            structure: SourceStructure::new(masked_source),
            import_prefixes: analysis
                .imports
                .iter()
                .filter_map(|import| import.prefix.as_deref())
                .collect(),
            local_functions: RefCell::new(HashMap::new()),
        }
    }

    /// Whether some import of the file has the prefix `name`.
    pub(crate) fn is_import_prefix(&self, name: &str) -> bool {
        self.import_prefixes.contains(name)
    }

    /// Whether `name` is declared as a local function in `callable`; `scan` finds all such names of
    /// the callable and runs at most once for each callable.
    pub(crate) fn declares_local_function(
        &self,
        callable: &DartDeclaration,
        name: &str,
        scan: impl FnOnce() -> HashSet<String>,
    ) -> bool {
        let key = std::ptr::from_ref(callable) as usize;
        self.local_functions
            .borrow_mut()
            .entry(key)
            .or_insert_with(scan)
            .contains(name)
    }
}
