use std::collections::HashMap;

use dartscope_core::{
    DartDiagnostic, DartFileAnalysis, DartFileInput, DartFileReferenceAnalysis,
    DartLibraryDirective, DartPart, DartPartOf, DartProjectAnalysis, DartProjectInput,
    DartProjectReferenceAnalysis, DartProjectSummary, SourceSpan, normalize_path,
};
use dartscope_resolve::parse_package_config;

use crate::backend::{DartParser, HeuristicDartParser};
use crate::declaration_inventory::collect_declaration_inventory;
use crate::declarations::{
    directive_like_without_semicolon, library_directive_name, part_of_value,
    string_constant_from_line,
};
use crate::graphql::{extract_graphql_operation_uses, extract_graphql_operations};
use crate::identifier_references::{collect_identifier_references, sort_identifier_references};
use crate::invocations::collect_invocations;
use crate::lexical::mask_non_code;
use crate::lexical_bindings::{collect_lexical_bindings, sort_lexical_bindings};
use crate::lexical_reads::collect_lexical_read_references;
use crate::lexical_writes::collect_lexical_write_references;
use crate::namespace::{directive_uri, extract_namespace_directives};
use crate::pubspec::parse_pubspec;
use crate::source_lines::{SourceLine, attach_diagnostic_paths, source_lines};

pub(crate) fn analyze_file_heuristic(input: DartFileInput) -> DartFileAnalysis {
    let lexical = mask_non_code(&input.source);
    let mut state = FileAnalysisState::new(&input, &lexical.code, lexical.diagnostics);
    for (source_line, code_line) in source_lines(&input.source)
        .into_iter()
        .zip(source_lines(&lexical.code))
    {
        state.observe_line(source_line, code_line);
    }
    state.finish()
}

struct FileAnalysisState {
    analysis: DartFileAnalysis,
    source: String,
    masked_source: String,
}

impl FileAnalysisState {
    fn new(
        input: &DartFileInput,
        masked_source: &str,
        lexical_diagnostics: Vec<DartDiagnostic>,
    ) -> Self {
        let mut analysis = DartFileAnalysis::empty(input.path.clone());
        analysis.graphql_operations = extract_graphql_operations(&input.source, masked_source);
        analysis.graphql_operation_uses =
            extract_graphql_operation_uses(&input.source, masked_source);
        let (imports, exports, mut diagnostics) =
            extract_namespace_directives(&input.source, masked_source);
        diagnostics.extend(lexical_diagnostics);
        analysis.imports = imports;
        analysis.exports = exports;
        analysis.diagnostics = diagnostics;

        Self {
            analysis,
            source: input.source.clone(),
            masked_source: masked_source.to_string(),
        }
    }

    fn observe_line(&mut self, source_line: SourceLine<'_>, code_line: SourceLine<'_>) {
        let line = source_line.text;
        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);
        let source_trimmed = line.trim();
        let code_trimmed = code_line.text.trim();
        let indent = code_line
            .text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();

        self.observe_merge_conflict(code_trimmed, &span);
        self.observe_dart_item(code_trimmed, source_trimmed, indent, &span);

        if directive_like_without_semicolon(code_trimmed) {
            self.analysis.diagnostics.push(DartDiagnostic::warning(
                "directive_missing_semicolon",
                "Dart import/export/part directive appears to be missing a semicolon",
                Some(span),
            ));
        }
    }

    fn observe_merge_conflict(&mut self, trimmed: &str, span: &SourceSpan) {
        if trimmed.contains("<<<<<<<") || trimmed.contains(">>>>>>>") {
            self.analysis.diagnostics.push(DartDiagnostic::warning(
                "merge_conflict_marker",
                "source contains a merge conflict marker",
                Some(span.clone()),
            ));
        }
    }

    fn observe_dart_item(
        &mut self,
        code_trimmed: &str,
        source_trimmed: &str,
        indent: usize,
        span: &SourceSpan,
    ) {
        if let Some(name) = library_directive_name(code_trimmed) {
            self.analysis.library = Some(DartLibraryDirective {
                name,
                span: span.clone(),
            });
        } else if starts_keyword(code_trimmed, "part of") {
            if let Some((library, kind)) = part_of_value(source_trimmed) {
                self.analysis.part_of = Some(DartPartOf {
                    library,
                    kind,
                    span: span.clone(),
                });
            }
        } else if starts_keyword(code_trimmed, "part")
            && let Some(uri) = directive_uri(source_trimmed, "part")
        {
            self.analysis.parts.push(DartPart {
                uri,
                span: span.clone(),
            });
        }

        if let Some(constant) = string_constant_from_line(source_trimmed, indent, span.clone()) {
            self.analysis.string_constants.push(constant);
        }
    }

    fn finish(mut self) -> DartFileAnalysis {
        let (declarations, diagnostics) =
            collect_declaration_inventory(&self.analysis.path, &self.source, &self.masked_source);
        self.analysis.declarations = declarations;
        self.analysis.invocations = collect_invocations(
            &self.source,
            &self.masked_source,
            &self.analysis.declarations,
        );
        self.analysis.diagnostics.extend(diagnostics);
        attach_diagnostic_paths(&mut self.analysis.diagnostics, &self.analysis.path);
        self.analysis
    }
}

fn starts_keyword(line: &str, keyword: &str) -> bool {
    line == keyword
        || line
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

pub(crate) fn analyze_project_with_backend(
    parser: &dyn DartParser,
    input: DartProjectInput,
) -> DartProjectAnalysis {
    let root = normalize_path(input.root);
    let mut files: Vec<_> = input
        .files
        .into_iter()
        .map(|file| parser.analyze_file(file))
        .collect();
    let mut pubspecs: Vec<_> = input.pubspecs.into_iter().map(parse_pubspec).collect();
    let mut package_configs: Vec<_> = input
        .package_configs
        .into_iter()
        .map(parse_package_config)
        .collect();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    pubspecs.sort_by(|left, right| left.path.cmp(&right.path));
    package_configs.sort_by(|left, right| left.path.cmp(&right.path));

    let file_diagnostics = files
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter().cloned());
    let pubspec_diagnostics = pubspecs
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter().cloned());
    let package_config_diagnostics = package_configs
        .iter()
        .flat_map(|analysis| analysis.diagnostics.iter().cloned());
    let diagnostics: Vec<_> = file_diagnostics
        .chain(pubspec_diagnostics)
        .chain(package_config_diagnostics)
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
        root,
        files,
        pubspecs,
        package_configs,
        summary,
        diagnostics,
    }
}

pub fn analyze_file(input: DartFileInput) -> DartFileAnalysis {
    HeuristicDartParser.analyze_file(input)
}

pub fn analyze_project(input: DartProjectInput) -> DartProjectAnalysis {
    analyze_project_with_backend(&HeuristicDartParser, input)
}

/// Analyzes one file and opt-in conservative reference and lexical-binding facts.
pub fn analyze_file_with_references(input: DartFileInput) -> DartFileReferenceAnalysis {
    let source = input.source.clone();
    let file = analyze_file(input);
    let lexical = mask_non_code(&source);
    let bindings = collect_lexical_bindings(&source, &lexical.code, &file);
    let mut references = collect_identifier_references(&source, &lexical.code, &file);
    let lexical_reads =
        collect_lexical_read_references(&source, &lexical.code, &file, &bindings, &references);
    references.extend(lexical_reads);
    let lexical_writes =
        collect_lexical_write_references(&source, &lexical.code, &file, &bindings, &references);
    references.extend(lexical_writes);
    sort_identifier_references(&mut references);
    DartFileReferenceAnalysis {
        file,
        references,
        bindings,
    }
}

/// Analyzes a project and opt-in conservative reference and lexical-binding facts.
pub fn analyze_project_with_references(input: DartProjectInput) -> DartProjectReferenceAnalysis {
    let sources: HashMap<_, _> = input
        .files
        .iter()
        .map(|file| (normalize_path(file.path.clone()), file.source.clone()))
        .collect();
    let project = analyze_project(input);
    let mut references = Vec::new();
    let mut bindings = Vec::new();
    for file in &project.files {
        let Some(source) = sources.get(&file.path) else {
            continue;
        };
        let lexical = mask_non_code(source);
        let file_bindings = collect_lexical_bindings(source, &lexical.code, file);
        let mut file_references = collect_identifier_references(source, &lexical.code, file);
        let lexical_reads = collect_lexical_read_references(
            source,
            &lexical.code,
            file,
            &file_bindings,
            &file_references,
        );
        file_references.extend(lexical_reads);
        let lexical_writes = collect_lexical_write_references(
            source,
            &lexical.code,
            file,
            &file_bindings,
            &file_references,
        );
        file_references.extend(lexical_writes);
        references.extend(file_references);
        bindings.extend(file_bindings);
    }
    sort_identifier_references(&mut references);
    sort_lexical_bindings(&mut bindings);
    DartProjectReferenceAnalysis {
        project,
        references,
        bindings,
    }
}
