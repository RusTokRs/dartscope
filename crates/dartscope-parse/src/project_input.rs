use std::collections::BTreeMap;

use dartscope_core::{
    DartDiagnostic, DartFileInput, DartProjectAnalysis, DartProjectInput, PackageConfigInput,
    PubspecInput, normalize_path,
};

pub(crate) fn prepare_project_input(
    mut input: DartProjectInput,
) -> (DartProjectInput, Vec<DartDiagnostic>) {
    let (files, mut diagnostics) = deduplicate_entries(input.files, "Dart file");
    let (pubspecs, pubspec_diagnostics) = deduplicate_entries(input.pubspecs, "pubspec");
    let (package_configs, package_config_diagnostics) =
        deduplicate_entries(input.package_configs, "package config");

    diagnostics.extend(pubspec_diagnostics);
    diagnostics.extend(package_config_diagnostics);

    input.files = files;
    input.pubspecs = pubspecs;
    input.package_configs = package_configs;

    (input, diagnostics)
}

pub(crate) fn append_project_diagnostics(
    analysis: &mut DartProjectAnalysis,
    mut diagnostics: Vec<DartDiagnostic>,
) {
    analysis.diagnostics.append(&mut diagnostics);
    analysis.summary.diagnostics = analysis.diagnostics.len();
}

trait ProjectInputEntry {
    fn path(&self) -> &str;
    fn source(&self) -> &str;
    fn set_path(&mut self, path: String);
}

impl ProjectInputEntry for DartFileInput {
    fn path(&self) -> &str {
        &self.path
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn set_path(&mut self, path: String) {
        self.path = path;
    }
}

impl ProjectInputEntry for PubspecInput {
    fn path(&self) -> &str {
        &self.path
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn set_path(&mut self, path: String) {
        self.path = path;
    }
}

impl ProjectInputEntry for PackageConfigInput {
    fn path(&self) -> &str {
        &self.path
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn set_path(&mut self, path: String) {
        self.path = path;
    }
}

fn deduplicate_entries<T: ProjectInputEntry>(
    entries: Vec<T>,
    input_kind: &str,
) -> (Vec<T>, Vec<DartDiagnostic>) {
    let mut by_path: BTreeMap<String, Vec<T>> = BTreeMap::new();

    for mut entry in entries {
        let path = normalize_path(entry.path().to_string());
        entry.set_path(path.clone());
        by_path.entry(path).or_default().push(entry);
    }

    let mut unique = Vec::new();
    let mut diagnostics = Vec::new();

    for (path, mut candidates) in by_path {
        let count = candidates.len();
        if count == 1 {
            unique.push(candidates.pop().expect("one project input candidate"));
            continue;
        }

        let first_source = candidates[0].source().to_string();
        let identical = candidates
            .iter()
            .all(|candidate| candidate.source() == first_source.as_str());

        if identical {
            unique.push(candidates.pop().expect("duplicate project input candidate"));
            diagnostics.push(
                DartDiagnostic::warning(
                    "duplicate_project_input_path",
                    format!(
                        "{input_kind} input path `{path}` appeared {count} times with identical source; duplicate entries were ignored"
                    ),
                    None,
                )
                .with_path(path),
            );
        } else {
            diagnostics.push(
                DartDiagnostic::error(
                    "duplicate_project_input_path",
                    format!(
                        "{input_kind} input path `{path}` appeared {count} times with conflicting source contents; all colliding entries were skipped"
                    ),
                    None,
                )
                .with_path(path),
            );
        }
    }

    (unique, diagnostics)
}
