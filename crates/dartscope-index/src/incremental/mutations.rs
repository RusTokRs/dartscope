//! The mutation API of the index: files, pubspecs, package configurations, options and root.

use std::collections::BTreeSet;

use dartscope_core::{
    DartFileAnalysis, DartFileReferenceAnalysis, DartIdentifierReference, DartLexicalBinding,
    PackageConfigAnalysis, PubspecAnalysis, normalize_path,
};

use crate::uri_graph::DartIndexOptions;

use super::plan::{
    RebuildPlan, changed_graphql_operation_names, declaration_names_to_refresh, file_rebuild_plan,
};
use super::project::{
    normalize_bindings_for_path, normalize_file, normalize_package_config, normalize_pubspec,
    normalize_references_for_path,
};
use super::{DartWorkspaceIndex, DartWorkspaceSubsystems, DartWorkspaceUpdate};

impl DartWorkspaceIndex {
    /// Inserts or replaces a normalized file analysis and clears stale reference facts for that path.
    pub fn upsert_file(&mut self, file: DartFileAnalysis) -> DartWorkspaceUpdate {
        self.upsert_file_internal(file, None, None)
    }

    /// Inserts or replaces a file together with parser-produced identifier-reference facts.
    pub fn upsert_file_with_references(
        &mut self,
        analysis: DartFileReferenceAnalysis,
    ) -> DartWorkspaceUpdate {
        self.upsert_file_internal(
            analysis.file,
            Some(analysis.references),
            Some(analysis.bindings),
        )
    }

    fn upsert_file_internal(
        &mut self,
        file: DartFileAnalysis,
        references: Option<Vec<DartIdentifierReference>>,
        bindings: Option<Vec<DartLexicalBinding>>,
    ) -> DartWorkspaceUpdate {
        let file = normalize_file(file);
        let path = file.path.clone();
        let new_references = references
            .map(|references| normalize_references_for_path(&path, references))
            .unwrap_or_default();
        let new_bindings = bindings
            .map(|bindings| normalize_bindings_for_path(&path, bindings))
            .unwrap_or_default();
        let old_file = self.files.get(&path).cloned();
        let old_references = self
            .references_by_path
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let old_bindings = self
            .bindings_by_path
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let references_changed = old_references != new_references;
        let bindings_changed = old_bindings != new_bindings;

        if old_file.as_ref() == Some(&file) && !references_changed && !bindings_changed {
            return self.no_op_update();
        }

        let plan = match old_file.as_ref() {
            Some(old) => file_rebuild_plan(old, &file, references_changed || bindings_changed),
            None => RebuildPlan::all(),
        };
        let changed_declaration_names =
            declaration_names_to_refresh(old_file.as_ref(), Some(&file));
        let changed_graphql_operation_names =
            changed_graphql_operation_names(old_file.as_ref(), Some(&file));
        self.files.insert(path.clone(), file);
        if new_references.is_empty() {
            self.references_by_path.remove(&path);
        } else {
            self.references_by_path.insert(path.clone(), new_references);
        }
        if new_bindings.is_empty() {
            self.bindings_by_path.remove(&path);
        } else {
            self.bindings_by_path.insert(path.clone(), new_bindings);
        }
        self.rebuild(
            plan,
            BTreeSet::from([path]),
            false,
            old_file.is_none(),
            changed_declaration_names,
            changed_graphql_operation_names,
        )
    }

    /// Removes a file and its opt-in reference facts.
    pub fn remove_file(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        let Some(removed) = self.files.remove(&path) else {
            return self.no_op_update();
        };
        let changed_declaration_names = declaration_names_to_refresh(Some(&removed), None);
        let changed_graphql_operation_names = changed_graphql_operation_names(Some(&removed), None);
        self.references_by_path.remove(&path);
        self.bindings_by_path.remove(&path);
        self.rebuild(
            RebuildPlan::all(),
            BTreeSet::from([path]),
            false,
            true,
            changed_declaration_names,
            changed_graphql_operation_names,
        )
    }

    /// Inserts or replaces one pubspec analysis.
    pub fn upsert_pubspec(&mut self, pubspec: PubspecAnalysis) -> DartWorkspaceUpdate {
        let pubspec = normalize_pubspec(pubspec);
        let path = pubspec.path.clone();
        let old = self.pubspecs.get(&path).cloned();
        if old.as_ref() == Some(&pubspec) {
            return self.no_op_update();
        }
        let resolution_changed = old
            .as_ref()
            .map(|old| old.package_name != pubspec.package_name)
            .unwrap_or(true);
        self.pubspecs.insert(path.clone(), pubspec);
        self.rebuild(
            RebuildPlan::metadata(resolution_changed),
            BTreeSet::from([path]),
            resolution_changed,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Removes one pubspec analysis by normalized path.
    pub fn remove_pubspec(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.pubspecs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Inserts or replaces one parsed `.dart_tool/package_config.json` analysis.
    pub fn upsert_package_config(&mut self, config: PackageConfigAnalysis) -> DartWorkspaceUpdate {
        let config = normalize_package_config(config);
        let path = config.path.clone();
        if self.package_configs.get(&path) == Some(&config) {
            return self.no_op_update();
        }
        self.package_configs.insert(path.clone(), config);
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Removes one package configuration by normalized path.
    pub fn remove_package_config(&mut self, path: &str) -> DartWorkspaceUpdate {
        let path = normalize_path(path.to_string());
        if self.package_configs.remove(&path).is_none() {
            return self.no_op_update();
        }
        self.rebuild(
            RebuildPlan::metadata(true),
            BTreeSet::from([path]),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Replaces conditional-compilation options without changing normalized project inputs.
    pub fn update_options(&mut self, options: DartIndexOptions) -> DartWorkspaceUpdate {
        if self.options == options {
            return self.no_op_update();
        }
        self.options = options;
        self.rebuild(
            RebuildPlan::options(),
            BTreeSet::new(),
            true,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    /// Changes only the informational project root retained in snapshots.
    pub fn update_root(&mut self, root: impl Into<String>) -> DartWorkspaceUpdate {
        let root = normalize_path(root.into());
        if self.root == root {
            return self.no_op_update();
        }
        self.root = root;
        self.rebuild(
            RebuildPlan::project_only(),
            BTreeSet::new(),
            false,
            false,
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    fn no_op_update(&mut self) -> DartWorkspaceUpdate {
        self.counters.no_op_updates += 1;
        DartWorkspaceUpdate {
            generation: self.snapshot.generation,
            changed_paths: Vec::new(),
            affected_paths: Vec::new(),
            affected_libraries: Vec::new(),
            rebuilt: DartWorkspaceSubsystems::default(),
        }
    }
}
