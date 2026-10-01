//! The project analysis that a snapshot exposes, and the normalization of its inputs.

use std::collections::BTreeMap;

use dartscope_core::{
    DartDiagnostic, DartFileAnalysis, DartIdentifierReference, DartLexicalBinding,
    DartProjectAnalysis, DartProjectSummary, PackageConfigAnalysis, PubspecAnalysis,
    normalize_path,
};

pub(super) fn build_project(
    root: &str,
    files: &BTreeMap<String, DartFileAnalysis>,
    pubspecs: &BTreeMap<String, PubspecAnalysis>,
    package_configs: &BTreeMap<String, PackageConfigAnalysis>,
    project_diagnostics: &[DartDiagnostic],
) -> DartProjectAnalysis {
    let files: Vec<_> = files.values().cloned().collect();
    let pubspecs: Vec<_> = pubspecs.values().cloned().collect();
    let package_configs: Vec<_> = package_configs.values().cloned().collect();
    let diagnostics: Vec<_> = files
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter().cloned())
        .chain(
            pubspecs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter().cloned()),
        )
        .chain(
            package_configs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter().cloned()),
        )
        .chain(project_diagnostics.iter().cloned())
        .collect();
    let summary = DartProjectSummary {
        dart_files: files.len(),
        pubspecs: pubspecs.len(),
        package_configs: package_configs.len(),
        imports: files.iter().map(|analysis| analysis.imports.len()).sum(),
        exports: files.iter().map(|analysis| analysis.exports.len()).sum(),
        parts: files.iter().map(|analysis| analysis.parts.len()).sum(),
        declarations: files
            .iter()
            .map(|analysis| analysis.declarations.len())
            .sum(),
        string_constants: files
            .iter()
            .map(|analysis| analysis.string_constants.len())
            .sum(),
        graphql_operations: files
            .iter()
            .map(|analysis| analysis.graphql_operations.len())
            .sum(),
        graphql_operation_uses: files
            .iter()
            .map(|analysis| analysis.graphql_operation_uses.len())
            .sum(),
        flutter_widgets: files
            .iter()
            .map(|analysis| analysis.flutter.widgets.len())
            .sum(),
        flutter_routes: files
            .iter()
            .map(|analysis| analysis.flutter.routes.len())
            .sum(),
        flutter_assets: files
            .iter()
            .map(|analysis| analysis.flutter.assets.len())
            .sum(),
        flutter_localizations: files
            .iter()
            .map(|analysis| analysis.flutter.localizations.len())
            .sum(),
        package_dependencies: pubspecs
            .iter()
            .map(|analysis| analysis.dependencies.len())
            .sum(),
        diagnostics: diagnostics.len(),
    };

    DartProjectAnalysis {
        root: root.to_string(),
        files,
        pubspecs,
        package_configs,
        summary,
        diagnostics,
    }
}

pub(super) fn additional_project_diagnostics(project: &DartProjectAnalysis) -> Vec<DartDiagnostic> {
    let child_diagnostics: Vec<_> = project
        .files
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter())
        .chain(
            project
                .pubspecs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter()),
        )
        .chain(
            project
                .package_configs
                .iter()
                .flat_map(|analysis| analysis.diagnostics.iter()),
        )
        .collect();
    let mut consumed = vec![false; child_diagnostics.len()];
    let mut additional = Vec::new();
    for diagnostic in &project.diagnostics {
        let matched = child_diagnostics
            .iter()
            .enumerate()
            .find(|(index, candidate)| !consumed[*index] && **candidate == diagnostic)
            .map(|(index, _)| index);
        if let Some(index) = matched {
            consumed[index] = true;
        } else {
            additional.push(diagnostic.clone());
        }
    }
    additional
}

pub(super) fn normalize_file(mut file: DartFileAnalysis) -> DartFileAnalysis {
    file.path = normalize_path(file.path);
    file
}

pub(super) fn normalize_pubspec(mut pubspec: PubspecAnalysis) -> PubspecAnalysis {
    pubspec.path = normalize_path(pubspec.path);
    pubspec
}

pub(super) fn normalize_package_config(mut config: PackageConfigAnalysis) -> PackageConfigAnalysis {
    config.path = normalize_path(config.path);
    config
}

pub(super) fn aggregate_references(
    references_by_path: &BTreeMap<String, Vec<DartIdentifierReference>>,
) -> Vec<DartIdentifierReference> {
    references_by_path
        .values()
        .flat_map(|references| references.iter().cloned())
        .collect()
}

pub(super) fn aggregate_bindings(
    bindings_by_path: &BTreeMap<String, Vec<DartLexicalBinding>>,
) -> Vec<DartLexicalBinding> {
    bindings_by_path
        .values()
        .flat_map(|bindings| bindings.iter().cloned())
        .collect()
}

pub(super) fn group_bindings(
    bindings: Vec<DartLexicalBinding>,
) -> BTreeMap<String, Vec<DartLexicalBinding>> {
    let mut grouped: BTreeMap<String, Vec<DartLexicalBinding>> = BTreeMap::new();
    for mut binding in bindings {
        binding.source_path = normalize_path(binding.source_path);
        grouped
            .entry(binding.source_path.clone())
            .or_default()
            .push(binding);
    }
    for bindings in grouped.values_mut() {
        sort_and_deduplicate_bindings(bindings);
    }
    grouped
}

pub(super) fn group_references(
    references: Vec<DartIdentifierReference>,
) -> BTreeMap<String, Vec<DartIdentifierReference>> {
    let mut grouped: BTreeMap<String, Vec<DartIdentifierReference>> = BTreeMap::new();
    for mut reference in references {
        reference.source_path = normalize_path(reference.source_path);
        grouped
            .entry(reference.source_path.clone())
            .or_default()
            .push(reference);
    }
    for references in grouped.values_mut() {
        sort_and_deduplicate_references(references);
    }
    grouped
}

pub(super) fn normalize_references_for_path(
    path: &str,
    references: Vec<DartIdentifierReference>,
) -> Vec<DartIdentifierReference> {
    let mut references: Vec<_> = references
        .into_iter()
        .map(|mut reference| {
            reference.source_path = path.to_string();
            reference
        })
        .collect();
    sort_and_deduplicate_references(&mut references);
    references
}

pub(super) fn normalize_bindings_for_path(
    path: &str,
    bindings: Vec<DartLexicalBinding>,
) -> Vec<DartLexicalBinding> {
    let mut bindings: Vec<_> = bindings
        .into_iter()
        .map(|mut binding| {
            binding.source_path = path.to_string();
            binding
        })
        .collect();
    sort_and_deduplicate_bindings(&mut bindings);
    bindings
}

fn sort_and_deduplicate_bindings(bindings: &mut Vec<DartLexicalBinding>) {
    bindings.sort_by(|left, right| {
        (
            &left.source_path,
            left.declaration_span.byte_start,
            left.declaration_span.byte_end,
            left.kind,
            &left.name,
            &left.symbol_id,
        )
            .cmp(&(
                &right.source_path,
                right.declaration_span.byte_start,
                right.declaration_span.byte_end,
                right.kind,
                &right.name,
                &right.symbol_id,
            ))
    });
    bindings.dedup_by(|left, right| {
        left.source_path == right.source_path
            && left.symbol_id == right.symbol_id
            && left.declaration_span.byte_start == right.declaration_span.byte_start
            && left.declaration_span.byte_end == right.declaration_span.byte_end
    });
}

fn sort_and_deduplicate_references(references: &mut Vec<DartIdentifierReference>) {
    references.sort_by(|left, right| {
        (
            &left.source_path,
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.name,
            &left.prefix,
        )
            .cmp(&(
                &right.source_path,
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.name,
                &right.prefix,
            ))
    });
    references.dedup();
}
