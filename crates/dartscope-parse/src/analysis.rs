use std::collections::HashMap;

use dartscope_core::{
    Confidence, DartDiagnostic, DartFileAnalysis, DartFileInput, DartLibraryDirective, DartPart,
    DartPartOf, DartProjectAnalysis, DartProjectInput, DartProjectSummary, FlutterWidgetHint,
    SourceSpan, normalize_path,
};
use dartscope_resolve::parse_package_config;

use crate::backend::{DartParser, HeuristicDartParser};
use crate::declaration_inventory::collect_declaration_inventory;
use crate::declarations::{
    collect_string_constant_values, directive_like_without_semicolon, is_flutter_base,
    is_flutter_import, library_directive_name, part_of_value, string_constant_from_line,
};
use crate::flutter_hints::{
    PendingRouteHint, count_char, flutter_asset_from_line, flutter_localization_from_line,
    material_route_from_line, pending_route_from_line, route_constructor_is_complete,
    should_finish_route_hint, starts_material_routes_map,
};
use crate::graphql::{extract_graphql_operation_uses, extract_graphql_operations};
use crate::lexical::mask_non_code;
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
    pending_route: Option<PendingRouteHint>,
    material_routes_depth: Option<usize>,
    string_constants: HashMap<String, String>,
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
        analysis.flutter.imports_flutter = imports.iter().any(|item| is_flutter_import(&item.uri));
        analysis.imports = imports;
        analysis.exports = exports;
        analysis.diagnostics = diagnostics;

        Self {
            analysis,
            pending_route: None,
            material_routes_depth: None,
            string_constants: collect_string_constant_values(&input.source, masked_source),
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
        self.observe_flutter_references(code_trimmed, source_trimmed, &span);
        self.observe_material_routes(code_trimmed, source_trimmed, &span);
        self.observe_go_route(code_trimmed, source_trimmed, &span);
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

    fn observe_flutter_references(
        &mut self,
        code_trimmed: &str,
        source_trimmed: &str,
        span: &SourceSpan,
    ) {
        if let Some(asset) = flutter_asset_from_line(code_trimmed, source_trimmed, span.clone()) {
            self.analysis.flutter.assets.push(asset);
        }
        if let Some(localization) = flutter_localization_from_line(code_trimmed, span.clone()) {
            self.analysis.flutter.localizations.push(localization);
        }
    }

    fn observe_material_routes(
        &mut self,
        code_trimmed: &str,
        source_trimmed: &str,
        span: &SourceSpan,
    ) {
        if let Some(depth) = self.material_routes_depth.as_mut() {
            if let Some(route) =
                material_route_from_line(code_trimmed, source_trimmed, span.clone())
            {
                self.analysis.flutter.routes.push(route);
            }
            *depth = depth.saturating_add(count_char(code_trimmed, '{'));
            *depth = depth.saturating_sub(count_char(code_trimmed, '}'));
            if *depth == 0 {
                self.material_routes_depth = None;
            }
        } else if starts_material_routes_map(code_trimmed) {
            self.material_routes_depth =
                Some(count_char(code_trimmed, '{').saturating_sub(count_char(code_trimmed, '}')));
        }
    }

    fn observe_go_route(&mut self, code_trimmed: &str, source_trimmed: &str, span: &SourceSpan) {
        if let Some(route) = pending_route_from_line(
            code_trimmed,
            source_trimmed,
            span.clone(),
            &self.string_constants,
        ) {
            self.finish_pending_route();
            if route_constructor_is_complete(code_trimmed) {
                self.push_route(route);
            } else {
                self.pending_route = Some(route);
            }
            return;
        }

        if let Some(route) = self.pending_route.as_mut() {
            route.observe_line(code_trimmed, source_trimmed, &self.string_constants);
            if should_finish_route_hint(code_trimmed) {
                self.finish_pending_route();
            }
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

    fn push_route(&mut self, route: PendingRouteHint) {
        if let Some(route) = route.finish() {
            self.analysis.flutter.routes.push(route);
        }
    }

    fn finish_pending_route(&mut self) {
        if let Some(route) = self.pending_route.take() {
            self.push_route(route);
        }
    }

    fn finish(mut self) -> DartFileAnalysis {
        self.finish_pending_route();
        let (declarations, diagnostics) =
            collect_declaration_inventory(&self.analysis.path, &self.source, &self.masked_source);
        self.analysis.declarations = declarations;
        self.analysis.diagnostics.extend(diagnostics);
        for declaration in &self.analysis.declarations {
            if let Some(base_class) = declaration
                .extends
                .clone()
                .filter(|base| is_flutter_base(base))
            {
                self.analysis.flutter.widgets.push(FlutterWidgetHint {
                    class_name: declaration.name.clone(),
                    base_class,
                    confidence: Confidence::High,
                    span: declaration.span.clone(),
                });
            }
        }
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
