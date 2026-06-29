use dartscope_core::{
    normalize_path, Confidence, DartDeclaration, DartDeclarationKind, DartDiagnostic,
    DartEnclosingSymbol, DartEnclosingSymbolKind, DartExport, DartFileAnalysis, DartFileInput,
    DartGraphqlClientCall, DartGraphqlOperation, DartGraphqlOperationType, DartGraphqlOperationUse,
    DartImport, DartLibraryDirective, DartNamespaceCombinator, DartNamespaceCombinatorKind,
    DartPart, DartPartOf, DartPartOfKind, DartProjectAnalysis, DartProjectInput,
    DartProjectSummary, DartStringConstant, DartUriConfiguration, FlutterRouteHint,
    FlutterRoutePathKind, FlutterWidgetHint, PubspecAnalysis, PubspecDependency,
    PubspecDependencySection, PubspecInput, SourceSpan,
};

use std::collections::HashMap;

use dartscope_resolve::parse_package_config;

pub fn analyze_file(input: DartFileInput) -> DartFileAnalysis {
    let mut analysis = DartFileAnalysis::empty(input.path);
    analysis.graphql_operations = extract_graphql_operations(&input.source);
    analysis.graphql_operation_uses = extract_graphql_operation_uses(&input.source);
    let (imports, exports, directive_diagnostics) = extract_namespace_directives(&input.source);
    analysis.flutter.imports_flutter = imports.iter().any(|import| is_flutter_import(&import.uri));
    analysis.imports = imports;
    analysis.exports = exports;
    analysis.diagnostics = directive_diagnostics;
    let mut byte_offset = 0usize;
    let mut pending_route: Option<PendingRouteHint> = None;
    let mut material_routes_depth: Option<usize> = None;
    let mut string_constants = HashMap::new();

    for (index, line) in input.source.lines().enumerate() {
        let line_number = index + 1;
        let span = SourceSpan::line(line_number, byte_offset, line);
        let trimmed = line.trim();
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();

        if trimmed.contains("<<<<<<<") || trimmed.contains(">>>>>>>") {
            analysis.diagnostics.push(DartDiagnostic::warning(
                "merge_conflict_marker",
                "source contains a merge conflict marker",
                Some(span.clone()),
            ));
        }

        if let Some(depth) = material_routes_depth.as_mut() {
            if let Some(route_hint) = material_route_from_line(trimmed, span.clone()) {
                analysis.flutter.routes.push(route_hint);
            }
            *depth = depth.saturating_add(count_char(trimmed, '{'));
            *depth = depth.saturating_sub(count_char(trimmed, '}'));
            if *depth == 0 {
                material_routes_depth = None;
            }
        } else if starts_material_routes_map(trimmed) {
            material_routes_depth =
                Some(count_char(trimmed, '{').saturating_sub(count_char(trimmed, '}')));
        }

        if pending_route.is_none() {
            pending_route = pending_route_from_line(trimmed, span.clone(), &string_constants);
        } else if let Some(route) = pending_route.as_mut() {
            if route.path.is_none() {
                route.path = route_path_argument(trimmed, &string_constants);
            }
            if route.name.is_none() {
                route.name = route_name_argument(trimmed);
            }
            if should_finish_route_hint(trimmed) {
                if let Some(route_hint) = route.clone().finish() {
                    analysis.flutter.routes.push(route_hint);
                }
                pending_route = None;
            }
        }

        if let Some(name) = library_directive_name(trimmed) {
            analysis.library = Some(DartLibraryDirective {
                name,
                span: span.clone(),
            });
        } else if let Some((library, kind)) = part_of_value(trimmed) {
            analysis.part_of = Some(DartPartOf {
                library,
                kind,
                span: span.clone(),
            });
        } else if let Some(uri) = directive_uri(trimmed, "part") {
            analysis.parts.push(DartPart {
                uri,
                span: span.clone(),
            });
        } else if let Some(declaration) = declaration_from_line(trimmed, indent, span.clone()) {
            if let Some(base_class) = declaration
                .extends
                .clone()
                .filter(|base| is_flutter_base(base))
            {
                analysis.flutter.widgets.push(FlutterWidgetHint {
                    class_name: declaration.name.clone(),
                    base_class,
                    confidence: Confidence::High,
                    span: span.clone(),
                });
            }
            if let Some(string_constant) = string_constant_from_line(trimmed, indent, span.clone())
            {
                string_constants
                    .insert(string_constant.name.clone(), string_constant.value.clone());
                analysis.string_constants.push(string_constant);
            }
            analysis.declarations.push(declaration);
        }

        if directive_like_without_semicolon(trimmed) {
            analysis.diagnostics.push(DartDiagnostic::warning(
                "directive_missing_semicolon",
                "Dart import/export/part directive appears to be missing a semicolon",
                Some(span),
            ));
        }

        byte_offset += line.len() + 1;
    }

    if let Some(route_hint) = pending_route.and_then(PendingRouteHint::finish) {
        analysis.flutter.routes.push(route_hint);
    }

    analysis
}

struct PendingNamespaceDirective {
    keyword: &'static str,
    text: String,
    start_line: usize,
    start_byte: usize,
}

#[derive(Default)]
struct NamespaceDirectiveOutput {
    imports: Vec<DartImport>,
    exports: Vec<DartExport>,
    diagnostics: Vec<DartDiagnostic>,
}

fn extract_namespace_directives(
    source: &str,
) -> (Vec<DartImport>, Vec<DartExport>, Vec<DartDiagnostic>) {
    let mut output = NamespaceDirectiveOutput::default();
    let mut pending: Option<PendingNamespaceDirective> = None;
    let mut byte_offset = 0usize;
    let mut last_line = 1usize;
    let mut last_text = "";

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        last_line = line_number;
        last_text = line;

        if let Some(directive) = pending.as_mut() {
            directive.text.push(' ');
            directive.text.push_str(trimmed);
        } else if trimmed.starts_with("import ") {
            pending = Some(PendingNamespaceDirective {
                keyword: "import",
                text: trimmed.to_string(),
                start_line: line_number,
                start_byte: byte_offset,
            });
        } else if trimmed.starts_with("export ") {
            pending = Some(PendingNamespaceDirective {
                keyword: "export",
                text: trimmed.to_string(),
                start_line: line_number,
                start_byte: byte_offset,
            });
        }

        if trimmed.contains(';') {
            if let Some(directive) = pending.take() {
                finish_namespace_directive(
                    directive,
                    line_number,
                    byte_offset + line.len(),
                    line,
                    true,
                    &mut output,
                );
            }
        }

        byte_offset += line.len() + 1;
    }

    if let Some(directive) = pending {
        finish_namespace_directive(
            directive,
            last_line,
            byte_offset.saturating_sub(1),
            last_text,
            false,
            &mut output,
        );
    }

    (output.imports, output.exports, output.diagnostics)
}

fn finish_namespace_directive(
    pending: PendingNamespaceDirective,
    end_line: usize,
    end_byte: usize,
    end_text: &str,
    terminated: bool,
    output: &mut NamespaceDirectiveOutput,
) {
    let span = SourceSpan {
        byte_start: pending.start_byte,
        byte_end: end_byte,
        start_line: pending.start_line,
        start_column: 1,
        end_line,
        end_column: end_text.chars().count() + 1,
    };
    if let Some(directive) = namespace_directive(&pending.text, pending.keyword) {
        if pending.keyword == "import" {
            output.imports.push(DartImport {
                uri: directive.uri,
                configurations: directive.configurations,
                is_deferred: directive.is_deferred,
                prefix: directive.prefix,
                combinators: directive.combinators,
                span: span.clone(),
            });
        } else {
            output.exports.push(DartExport {
                uri: directive.uri,
                configurations: directive.configurations,
                combinators: directive.combinators,
                span: span.clone(),
            });
        }
    }
    if !terminated {
        output.diagnostics.push(DartDiagnostic::warning(
            "directive_missing_semicolon",
            "Dart import/export directive appears to be missing a semicolon",
            Some(span),
        ));
    }
}

pub fn analyze_project(input: DartProjectInput) -> DartProjectAnalysis {
    let root = normalize_path(input.root);
    let files: Vec<_> = input.files.into_iter().map(analyze_file).collect();
    let pubspecs: Vec<_> = input.pubspecs.into_iter().map(parse_pubspec).collect();
    let package_configs: Vec<_> = input
        .package_configs
        .into_iter()
        .map(parse_package_config)
        .collect();

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

pub fn parse_pubspec(input: PubspecInput) -> PubspecAnalysis {
    let mut analysis = PubspecAnalysis {
        path: normalize_path(input.path),
        package_name: None,
        dependencies: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut section: Option<PubspecDependencySection> = None;
    let mut byte_offset = 0usize;

    for (index, line) in input.source.lines().enumerate() {
        let line_number = index + 1;
        let span = SourceSpan::line(line_number, byte_offset, line);
        let trimmed = line.trim();
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            byte_offset += line.len() + 1;
            continue;
        }

        if indent == 0 {
            section = match trimmed.trim_end_matches(':') {
                "dependencies" => Some(PubspecDependencySection::Dependencies),
                "dev_dependencies" => Some(PubspecDependencySection::DevDependencies),
                "dependency_overrides" => Some(PubspecDependencySection::DependencyOverrides),
                _ => None,
            };
            if let Some(value) = key_value(trimmed, "name") {
                analysis.package_name = Some(value.to_string());
            }
        } else if let Some(section) = section.filter(|_| indent <= 2) {
            if let Some((name, value)) = yaml_key_value(trimmed) {
                analysis.dependencies.push(PubspecDependency {
                    name: name.to_string(),
                    section,
                    version_or_source: value.map(str::to_string),
                    span,
                });
            }
        }

        byte_offset += line.len() + 1;
    }

    if analysis.package_name.is_none() {
        analysis.diagnostics.push(DartDiagnostic::warning(
            "pubspec_missing_name",
            "pubspec.yaml does not declare a package name",
            None,
        ));
    }

    analysis
}

#[derive(Debug, Clone)]
struct PendingRouteHint {
    constructor: String,
    span: SourceSpan,
    path: Option<RoutePathValue>,
    name: Option<String>,
}

impl PendingRouteHint {
    fn finish(self) -> Option<FlutterRouteHint> {
        let path = self.path?;
        Some(FlutterRouteHint {
            constructor: self.constructor,
            path: path.value,
            path_kind: path.kind,
            resolved_path: path.resolved_value,
            name: self.name,
            confidence: path.confidence,
            span: self.span,
        })
    }
}

#[derive(Debug, Clone)]
struct RoutePathValue {
    value: String,
    kind: FlutterRoutePathKind,
    resolved_value: Option<String>,
    confidence: Confidence,
}

fn directive_uri(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    quoted_value(rest)
}

#[derive(Debug, Default)]
struct ParsedNamespaceDirective {
    uri: String,
    configurations: Vec<DartUriConfiguration>,
    is_deferred: bool,
    prefix: Option<String>,
    combinators: Vec<DartNamespaceCombinator>,
}

fn namespace_directive(trimmed: &str, keyword: &str) -> Option<ParsedNamespaceDirective> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let (uri, suffix) = quoted_value_with_suffix(rest)?;
    let (configurations, suffix) = uri_configurations(suffix);
    let tokens = directive_suffix_tokens(suffix);
    let mut directive = ParsedNamespaceDirective {
        uri,
        configurations,
        ..ParsedNamespaceDirective::default()
    };
    let mut index = 0;

    while index < tokens.len() {
        match tokens[index] {
            "deferred" if keyword == "import" => {
                directive.is_deferred = true;
                index += 1;
            }
            "as" if keyword == "import" => {
                directive.prefix = tokens
                    .get(index + 1)
                    .filter(|name| is_identifier(name))
                    .map(|name| (*name).to_string());
                index += 2;
            }
            "show" | "hide" => {
                let kind = if tokens[index] == "show" {
                    DartNamespaceCombinatorKind::Show
                } else {
                    DartNamespaceCombinatorKind::Hide
                };
                index += 1;
                let start = index;
                while index < tokens.len() && !matches!(tokens[index], "show" | "hide") {
                    index += 1;
                }
                let names = tokens[start..index]
                    .iter()
                    .filter(|name| is_identifier(name))
                    .map(|name| (*name).to_string())
                    .collect();
                directive
                    .combinators
                    .push(DartNamespaceCombinator { kind, names });
            }
            _ => index += 1,
        }
    }

    Some(directive)
}

fn uri_configurations(mut suffix: &str) -> (Vec<DartUriConfiguration>, &str) {
    let mut configurations = Vec::new();
    loop {
        suffix = suffix.trim_start();
        let Some(after_if) = suffix.strip_prefix("if") else {
            break;
        };
        if !after_if.starts_with(char::is_whitespace) && !after_if.starts_with('(') {
            break;
        }
        let after_if = after_if.trim_start();
        let Some(condition_start) = after_if.strip_prefix('(') else {
            break;
        };
        let Some(condition_end) = condition_start.find(')') else {
            break;
        };
        let condition = condition_start[..condition_end].trim();
        if condition.is_empty() {
            break;
        }
        let after_condition = &condition_start[condition_end + 1..];
        let Some((uri, remaining)) = quoted_value_with_suffix(after_condition) else {
            break;
        };
        configurations.push(DartUriConfiguration {
            condition: condition.to_string(),
            uri,
        });
        suffix = remaining;
    }
    (configurations, suffix)
}

fn quoted_value_with_suffix(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let quote_index = usize::from(input.starts_with('r'));
    let quote = *input.as_bytes().get(quote_index)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let rest = &input[quote_index + 1..];
    let end = rest.find(quote as char)?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

fn directive_suffix_tokens(suffix: &str) -> Vec<&str> {
    suffix
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';'))
        .filter(|token| !token.is_empty())
        .collect()
}

fn pending_route_from_line(
    trimmed: &str,
    span: SourceSpan,
    string_constants: &HashMap<String, String>,
) -> Option<PendingRouteHint> {
    let constructor = if trimmed.starts_with("GoRoute(") {
        "GoRoute"
    } else {
        return None;
    };

    Some(PendingRouteHint {
        constructor: constructor.to_string(),
        span,
        path: route_path_argument(trimmed, string_constants),
        name: route_name_argument(trimmed),
    })
}

fn route_path_argument(
    trimmed: &str,
    string_constants: &HashMap<String, String>,
) -> Option<RoutePathValue> {
    named_argument_value(trimmed, "path").map(|value| route_path_value(value, string_constants))
}

fn route_name_argument(trimmed: &str) -> Option<String> {
    named_argument_value(trimmed, "name").map(|value| {
        quoted_value(value).unwrap_or_else(|| value.trim_end_matches(',').trim().to_string())
    })
}

fn starts_material_routes_map(trimmed: &str) -> bool {
    trimmed.starts_with("routes:") && trimmed.contains('{')
}

fn material_route_from_line(trimmed: &str, span: SourceSpan) -> Option<FlutterRouteHint> {
    let (key, _) = trimmed.split_once(':')?;
    let path = quoted_value(key.trim())?;
    Some(FlutterRouteHint {
        constructor: "MaterialApp.routes".to_string(),
        path: path.clone(),
        path_kind: FlutterRoutePathKind::Literal,
        resolved_path: Some(path),
        name: None,
        confidence: Confidence::High,
        span,
    })
}

fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

fn named_argument_value<'a>(trimmed: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}:");
    trimmed
        .strip_prefix(&marker)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn route_path_value(value: &str, string_constants: &HashMap<String, String>) -> RoutePathValue {
    let value = value.trim_end_matches(',').trim();
    if let Some(literal) = quoted_value(value) {
        let resolved_literal = resolve_interpolated_string(&literal, string_constants);
        let is_interpolated = literal.contains('$');
        let is_unresolved_interpolation = is_interpolated && resolved_literal.is_none();
        return RoutePathValue {
            value: literal,
            kind: if is_interpolated {
                FlutterRoutePathKind::Expression
            } else {
                FlutterRoutePathKind::Literal
            },
            resolved_value: resolved_literal,
            confidence: if is_unresolved_interpolation {
                Confidence::Medium
            } else {
                Confidence::High
            },
        };
    }

    if let Some(resolved_value) = string_constants.get(value) {
        return RoutePathValue {
            value: value.to_string(),
            kind: FlutterRoutePathKind::Expression,
            resolved_value: Some(resolved_value.clone()),
            confidence: Confidence::High,
        };
    }

    RoutePathValue {
        value: value.to_string(),
        kind: FlutterRoutePathKind::Expression,
        resolved_value: None,
        confidence: Confidence::Medium,
    }
}

fn should_finish_route_hint(trimmed: &str) -> bool {
    trimmed.starts_with("builder:")
        || trimmed.starts_with("pageBuilder:")
        || trimmed.starts_with("redirect:")
        || trimmed.starts_with("routes:")
        || trimmed == "),"
        || trimmed == ")"
}

fn extract_graphql_operations(source: &str) -> Vec<DartGraphqlOperation> {
    let mut operations = Vec::new();
    let mut lines = source.lines().enumerate();
    let mut byte_offset = 0usize;

    while let Some((index, line)) = lines.next() {
        let line_number = index + 1;
        let trimmed = line.trim();
        let Some((constant_name, delimiter)) = graphql_document_start(trimmed) else {
            byte_offset += line.len() + 1;
            continue;
        };

        let span = SourceSpan::line(line_number, byte_offset, line);
        let mut document = String::new();
        let mut ended_on_start_line = false;
        if let Some(after_start) = trimmed.split_once(delimiter).map(|(_, right)| right) {
            if let Some((before_end, _)) = after_start.split_once(delimiter) {
                document.push_str(before_end);
                ended_on_start_line = true;
            } else {
                document.push_str(after_start);
                document.push('\n');
            }
        }

        byte_offset += line.len() + 1;
        if !ended_on_start_line {
            for (_, document_line) in lines.by_ref() {
                if let Some((before_end, _)) = document_line.split_once(delimiter) {
                    document.push_str(before_end);
                    byte_offset += document_line.len() + 1;
                    break;
                }
                document.push_str(document_line);
                document.push('\n');
                byte_offset += document_line.len() + 1;
            }
        }

        if let Some(operation) = graphql_operation_from_document(constant_name, &document, span) {
            operations.push(operation);
        }
    }

    operations
}

fn graphql_document_start(trimmed: &str) -> Option<(String, &'static str)> {
    if !trimmed.starts_with("const ") && !trimmed.starts_with("final ") {
        return None;
    }
    let (left, right) = trimmed.split_once('=')?;
    let constant_name = variable_name_from_declaration_left(left)?;
    let right = right.trim_start();
    let delimiter = if right.starts_with("r'''") || right.starts_with("'''") {
        "'''"
    } else if right.starts_with("r\"\"\"") || right.starts_with("\"\"\"") {
        "\"\"\""
    } else {
        return None;
    };
    Some((constant_name, delimiter))
}

fn variable_name_from_declaration_left(left: &str) -> Option<String> {
    ["const", "final"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(left.trim(), keyword))
}

fn graphql_operation_from_document(
    constant_name: String,
    document: &str,
    span: SourceSpan,
) -> Option<DartGraphqlOperation> {
    let document = document.trim();
    let (operation_type, rest) = if let Some(rest) = document.strip_prefix("query") {
        (DartGraphqlOperationType::Query, rest)
    } else if let Some(rest) = document.strip_prefix("mutation") {
        (DartGraphqlOperationType::Mutation, rest)
    } else if let Some(rest) = document.strip_prefix("subscription") {
        (DartGraphqlOperationType::Subscription, rest)
    } else {
        return None;
    };

    let operation_name = graphql_operation_name(rest);
    let variable_names = graphql_operation_variable_names(rest);
    let root_fields = graphql_root_fields(document);

    Some(DartGraphqlOperation {
        constant_name,
        operation_type,
        operation_name,
        variable_names,
        root_fields,
        span,
    })
}

fn graphql_operation_name(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    if rest.starts_with('{') || rest.is_empty() {
        return None;
    }
    next_identifier(rest)
}

fn graphql_operation_variable_names(rest: &str) -> Vec<String> {
    let Some(start) = rest.find('(') else {
        return Vec::new();
    };
    let before_selection = rest.find('{').unwrap_or(rest.len());
    if start > before_selection {
        return Vec::new();
    }
    let Some(end) = rest[start + 1..].find(')') else {
        return Vec::new();
    };
    let variables_block = &rest[start + 1..start + 1 + end];
    let mut variables = Vec::new();
    let mut chars = variables_block.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            continue;
        }
        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if !name.is_empty() {
            variables.push(name);
        }
    }

    variables.sort();
    variables.dedup();
    variables
}

fn graphql_root_fields(document: &str) -> Vec<String> {
    let Some(start) = document.find('{') else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    let mut depth = 0usize;
    let mut paren_depth = 0usize;
    let mut token = String::new();
    let mut chars = document[start..].chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                depth += 1;
                token.clear();
            }
            '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                token.clear();
            }
            '#' => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            ch if depth == 1 && (ch.is_ascii_alphabetic() || ch == '_') => {
                if paren_depth > 0 {
                    continue;
                }
                token.push(ch);
                while let Some(next) = chars.peek().copied() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        token.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !matches!(token.as_str(), "fragment" | "on") {
                    fields.push(token.clone());
                }
                token.clear();
            }
            '(' if depth == 1 => {
                paren_depth += 1;
                token.clear();
            }
            ')' if depth == 1 => {
                paren_depth = paren_depth.saturating_sub(1);
                token.clear();
            }
            _ => {
                token.clear();
            }
        }
    }

    fields.sort();
    fields.dedup();
    fields
}

fn extract_graphql_operation_uses(source: &str) -> Vec<DartGraphqlOperationUse> {
    let mut uses = Vec::new();
    let mut byte_offset = 0usize;
    let mut brace_depth = 0usize;
    let mut symbol_stack: Vec<(usize, DartEnclosingSymbol)> = Vec::new();
    let mut pending_symbol: Option<DartEnclosingSymbol> = None;
    let mut pending_client_call: Option<DartGraphqlClientCall> = None;
    let lines: Vec<_> = source.lines().collect();

    for (index, line) in lines.iter().enumerate() {
        let line_number = index + 1;
        let span = SourceSpan::line(line_number, byte_offset, line);
        let trimmed = line.trim();

        while symbol_stack
            .last()
            .is_some_and(|(parent_depth, _)| brace_depth <= *parent_depth)
        {
            symbol_stack.pop();
        }

        if let Some(callable) = callable_signature_name_from_line(trimmed) {
            pending_symbol = Some(DartEnclosingSymbol {
                name: callable,
                kind: DartEnclosingSymbolKind::Callable,
            });
        } else if brace_depth == 0 {
            if let Some(variable) = top_level_variable_owner_from_line(trimmed) {
                pending_symbol = Some(DartEnclosingSymbol {
                    name: variable,
                    kind: DartEnclosingSymbolKind::Variable,
                });
            }
        }
        if trimmed.ends_with('{') {
            let line_symbol = callable_name_from_line(trimmed).map(|name| DartEnclosingSymbol {
                name,
                kind: DartEnclosingSymbolKind::Callable,
            });
            if let Some(symbol) = line_symbol.or_else(|| pending_symbol.take()) {
                symbol_stack.push((brace_depth, symbol));
            }
        }

        if let Some(client_call) = graphql_client_call_from_line(trimmed) {
            pending_client_call = Some(client_call);
        }

        if let Some(constant_name) = gql_constant_from_line(trimmed) {
            let enclosing_symbol = symbol_stack.last().map(|(_, symbol)| symbol.clone());
            uses.push(DartGraphqlOperationUse {
                constant_name,
                client_call: pending_client_call.unwrap_or(DartGraphqlClientCall::Unknown),
                variable_names: graphql_variable_names_from_lines(&lines, index),
                enclosing_callable: enclosing_symbol
                    .as_ref()
                    .filter(|symbol| symbol.kind == DartEnclosingSymbolKind::Callable)
                    .map(|symbol| symbol.name.clone()),
                enclosing_symbol,
                span,
            });
            pending_client_call = None;
        }

        let opens = line.chars().filter(|c| *c == '{').count();
        let closes = line.chars().filter(|c| *c == '}').count();
        brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
        byte_offset += line.len() + 1;
    }

    uses
}

fn graphql_variable_names_from_lines(lines: &[&str], start_index: usize) -> Vec<String> {
    let mut variables = Vec::new();
    let mut in_variables = false;
    let mut map_depth = 0usize;

    for line in lines.iter().skip(start_index).take(80) {
        let trimmed = line.trim();
        let mut scan = trimmed;

        if !in_variables {
            let Some((_, after_marker)) = trimmed.split_once("variables:") else {
                if trimmed == ")," || trimmed == ")" {
                    break;
                }
                continue;
            };
            let Some(open_index) = after_marker.find('{') else {
                continue;
            };
            scan = &after_marker[open_index + 1..];
            in_variables = true;
            map_depth = 1;
        }

        collect_top_level_map_keys(scan, map_depth, &mut variables);
        for ch in scan.chars() {
            match ch {
                '{' => map_depth += 1,
                '}' => {
                    map_depth = map_depth.saturating_sub(1);
                    if map_depth == 0 {
                        variables.sort();
                        variables.dedup();
                        return variables;
                    }
                }
                _ => {}
            }
        }
    }

    variables.sort();
    variables.dedup();
    variables
}

fn collect_top_level_map_keys(line: &str, initial_depth: usize, variables: &mut Vec<String>) {
    let mut depth = initial_depth;
    let chars: Vec<_> = line.char_indices().collect();
    let mut index = 0usize;

    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        match ch {
            '\'' | '"' if depth == 1 => {
                if let Some((key, next_index)) = quoted_map_key_at(line, byte_index, ch) {
                    variables.push(key);
                    index = chars
                        .iter()
                        .position(|(candidate, _)| *candidate >= next_index)
                        .unwrap_or(chars.len());
                    continue;
                }
            }
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
}

fn quoted_map_key_at(line: &str, start: usize, quote: char) -> Option<(String, usize)> {
    let rest = &line[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    let key = &rest[..end];
    let after_key_index = start + quote.len_utf8() + end + quote.len_utf8();
    let after_key = line[after_key_index..].trim_start();
    after_key
        .starts_with(':')
        .then(|| (key.to_string(), after_key_index))
}

fn graphql_client_call_from_line(trimmed: &str) -> Option<DartGraphqlClientCall> {
    if trimmed.contains(".query(") {
        Some(DartGraphqlClientCall::Query)
    } else if trimmed.contains(".mutate(") {
        Some(DartGraphqlClientCall::Mutation)
    } else if trimmed.contains(".subscribe(") {
        Some(DartGraphqlClientCall::Subscription)
    } else {
        None
    }
}

fn gql_constant_from_line(trimmed: &str) -> Option<String> {
    let marker = "gql(";
    let start = trimmed.find(marker)? + marker.len();
    let rest = trimmed[start..].trim_start();
    if rest.starts_with('\'')
        || rest.starts_with('"')
        || rest.starts_with("r'")
        || rest.starts_with("r\"")
    {
        return None;
    }
    next_identifier(rest)
}

fn callable_name_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.ends_with('{') || !trimmed.contains('(') {
        return None;
    }
    callable_signature_name_from_line(trimmed)
}

fn callable_signature_name_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.contains('(') || trimmed.ends_with(';') || trimmed.contains("=>") {
        return None;
    }
    if trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("switch ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("builder:")
    {
        return None;
    }

    let before_paren = trimmed.split_once('(')?.0.trim();
    let name = before_paren.split_whitespace().last()?;
    is_identifier(name).then_some(name.to_string())
}

fn top_level_variable_owner_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.contains('=') {
        return None;
    }
    ["const", "final", "var"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(trimmed, keyword))
        .filter(|name| is_identifier(name))
}

fn library_directive_name(trimmed: &str) -> Option<Option<String>> {
    let rest = trimmed.strip_prefix("library")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) && !rest.starts_with(';') {
        return None;
    }
    let name = rest.trim().trim_end_matches(';').trim();
    if name.is_empty() {
        Some(None)
    } else {
        is_library_name(name).then(|| Some(name.to_string()))
    }
}

fn part_of_value(trimmed: &str) -> Option<(String, DartPartOfKind)> {
    let rest = trimmed.strip_prefix("part of")?.trim();
    quoted_value(rest)
        .map(|uri| (uri, DartPartOfKind::Uri))
        .or_else(|| {
            rest.trim_end_matches(';')
                .split_whitespace()
                .next()
                .filter(|name| is_library_name(name))
                .map(|name| (name.to_string(), DartPartOfKind::LibraryName))
        })
}

fn quoted_value(input: &str) -> Option<String> {
    let quote = input.find(['\'', '"'])?;
    let quote_char = input.as_bytes()[quote] as char;
    let rest = &input[quote + 1..];
    let end = rest.find(quote_char)?;
    Some(rest[..end].to_string())
}

fn declaration_from_line(
    trimmed: &str,
    indent: usize,
    span: SourceSpan,
) -> Option<DartDeclaration> {
    if let Some(name) = name_after_keyword(trimmed, "class") {
        return Some(DartDeclaration {
            name,
            kind: DartDeclarationKind::Class,
            span,
            extends: value_after_keyword(trimmed, "extends"),
            mixes_in: values_after_keyword(trimmed, "with"),
        });
    }
    if let Some(name) = name_after_keyword(trimmed, "mixin") {
        return Some(simple_declaration(name, DartDeclarationKind::Mixin, span));
    }
    if let Some(name) = name_after_keyword(trimmed, "enum") {
        return Some(simple_declaration(name, DartDeclarationKind::Enum, span));
    }
    if let Some(name) = name_after_keyword(trimmed, "extension") {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::Extension,
            span,
        ));
    }
    if let Some(name) = name_after_keyword(trimmed, "typedef") {
        return Some(simple_declaration(name, DartDeclarationKind::Typedef, span));
    }
    if let Some(name) = top_level_variable(trimmed, indent) {
        return Some(simple_declaration(
            name,
            DartDeclarationKind::Variable,
            span,
        ));
    }
    top_level_function(trimmed, indent)
        .map(|name| simple_declaration(name, DartDeclarationKind::Function, span))
}

fn simple_declaration(
    name: String,
    kind: DartDeclarationKind,
    span: SourceSpan,
) -> DartDeclaration {
    DartDeclaration {
        name,
        kind,
        span,
        extends: None,
        mixes_in: Vec::new(),
    }
}

fn name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    next_identifier(rest)
}

fn value_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let marker = format!(" {keyword} ");
    let index = trimmed.find(&marker)?;
    next_identifier(&trimmed[index + marker.len()..])
}

fn values_after_keyword(trimmed: &str, keyword: &str) -> Vec<String> {
    let marker = format!(" {keyword} ");
    let Some(index) = trimmed.find(&marker) else {
        return Vec::new();
    };
    trimmed[index + marker.len()..]
        .split(['{', '('])
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| next_identifier(part.trim()))
        .collect()
}

fn next_identifier(input: &str) -> Option<String> {
    let ident: String = input
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    (!ident.is_empty()).then_some(ident)
}

fn top_level_function(trimmed: &str, indent: usize) -> Option<String> {
    if indent != 0 {
        return None;
    }
    if !trimmed.ends_with('{') && !trimmed.ends_with("=>") && !trimmed.contains('(') {
        return None;
    }
    if trimmed
        .split_once('(')
        .is_some_and(|(before_paren, _)| before_paren.contains('='))
    {
        return None;
    }
    if trimmed.starts_with("if ") || trimmed.starts_with("for ") || trimmed.starts_with("while ") {
        return None;
    }
    let before_paren = trimmed.split_once('(')?.0.trim();
    let name = before_paren.split_whitespace().last()?;
    is_identifier(name).then_some(name.to_string())
}

fn top_level_variable(trimmed: &str, indent: usize) -> Option<String> {
    if indent != 0 {
        return None;
    }
    ["const", "final", "var"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(trimmed, keyword))
}

fn variable_name_after_keyword(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let before_equals = rest.split_once('=').map_or(rest, |(left, _)| left).trim();
    before_equals.split_whitespace().last().map(str::to_string)
}

fn string_constant_from_line(
    trimmed: &str,
    indent: usize,
    span: SourceSpan,
) -> Option<DartStringConstant> {
    if indent != 0 {
        return None;
    }
    let (left, right) = trimmed.trim_end_matches(';').split_once('=')?;
    let left = left.trim();
    let right = right.trim();
    let name = ["const", "final"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(left, keyword))?;
    let value = quoted_value(right)?;

    Some(DartStringConstant { name, value, span })
}

fn resolve_interpolated_string(
    value: &str,
    string_constants: &HashMap<String, String>,
) -> Option<String> {
    if !value.contains('$') {
        return Some(value.to_string());
    }

    let mut resolved = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            resolved.push(ch);
            continue;
        }

        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }

        if name.is_empty() {
            return None;
        }
        resolved.push_str(string_constants.get(&name)?);
    }

    Some(resolved)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_library_name(value: &str) -> bool {
    value.split('.').all(is_identifier)
}

fn is_flutter_base(base: &str) -> bool {
    matches!(
        base,
        "Widget"
            | "StatelessWidget"
            | "StatefulWidget"
            | "InheritedWidget"
            | "State"
            | "ConsumerWidget"
    )
}

fn is_flutter_import(uri: &str) -> bool {
    uri.starts_with("package:flutter/") || uri.starts_with("package:flutter_riverpod/")
}

fn directive_like_without_semicolon(trimmed: &str) -> bool {
    (trimmed.starts_with("part ") || trimmed.starts_with("part of ")) && !trimmed.ends_with(';')
}

fn key_value<'a>(trimmed: &'a str, key: &str) -> Option<&'a str> {
    trimmed
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn yaml_key_value(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let (key, value) = trimmed.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.starts_with('-') {
        return None;
    }
    let value = value.trim();
    Some((key, (!value.is_empty()).then_some(value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::{
        Confidence, DartDeclarationKind, DartEnclosingSymbolKind, DartGraphqlClientCall,
        DartProjectInput, FlutterRoutePathKind, PackageConfigInput, PubspecDependencySection,
    };

    #[test]
    fn analyzes_dart_imports_parts_declarations_and_flutter_widgets() {
        let source = r#"
import 'package:flutter/material.dart';
import 'src/model.dart';
export "src/api.dart";
part 'home.g.dart';

class HomeScreen extends StatelessWidget {
}

typedef Mapper = String Function(int value);
"#;

        let analysis = analyze_file(DartFileInput::new("lib\\home.dart", source));

        assert_eq!(analysis.path, "lib/home.dart");
        assert_eq!(analysis.imports.len(), 2);
        assert_eq!(analysis.exports[0].uri, "src/api.dart");
        assert_eq!(analysis.parts[0].uri, "home.g.dart");
        assert!(analysis.flutter.imports_flutter);
        assert_eq!(analysis.flutter.widgets[0].class_name, "HomeScreen");
        assert!(analysis
            .declarations
            .iter()
            .any(|declaration| declaration.name == "Mapper"
                && declaration.kind == DartDeclarationKind::Typedef));
    }

    #[test]
    fn parses_import_and_export_namespace_controls() {
        let source = r#"
import 'src/generated.dart' as generated show operation, model hide internal;
import 'src/lazy.dart' deferred as lazy;
export 'src/public.dart' show PublicApi hide InternalApi;
"#;

        let analysis = analyze_file(DartFileInput::new("lib/api.dart", source));

        assert_eq!(analysis.imports[0].prefix.as_deref(), Some("generated"));
        assert!(!analysis.imports[0].is_deferred);
        assert_eq!(analysis.imports[0].combinators.len(), 2);
        assert_eq!(
            analysis.imports[0].combinators[0].kind,
            DartNamespaceCombinatorKind::Show
        );
        assert_eq!(
            analysis.imports[0].combinators[0].names,
            ["operation", "model"]
        );
        assert_eq!(analysis.imports[0].combinators[1].names, ["internal"]);
        assert!(analysis.imports[1].is_deferred);
        assert_eq!(analysis.imports[1].prefix.as_deref(), Some("lazy"));
        assert_eq!(analysis.exports[0].combinators.len(), 2);
        assert_eq!(analysis.exports[0].combinators[0].names, ["PublicApi"]);
        assert_eq!(analysis.exports[0].combinators[1].names, ["InternalApi"]);
    }

    #[test]
    fn parses_conditional_imports_and_exports_without_selecting_a_platform() {
        let source = r#"
import 'src/stub.dart'
  if (dart.library.io) 'src/io.dart'
  if (dart.library.js_interop) 'src/web.dart' show PlatformApi;
export 'src/default.dart' if (dart.library.io) 'src/native.dart' hide Internal;
"#;

        let analysis = analyze_file(DartFileInput::new("lib/platform.dart", source));

        assert_eq!(analysis.imports.len(), 1);
        assert_eq!(analysis.imports[0].configurations.len(), 2);
        assert_eq!(analysis.imports[0].span.start_line, 2);
        assert_eq!(analysis.imports[0].span.end_line, 4);
        assert_eq!(analysis.imports[0].combinators[0].names, ["PlatformApi"]);
        assert_eq!(analysis.exports.len(), 1);
        assert_eq!(analysis.exports[0].configurations.len(), 1);
        assert_eq!(
            analysis.exports[0].configurations[0].condition,
            "dart.library.io"
        );
        assert_eq!(analysis.exports[0].configurations[0].uri, "src/native.dart");
        assert_eq!(analysis.exports[0].combinators[0].names, ["Internal"]);
        assert!(analysis.diagnostics.is_empty());

        let single_line = analyze_file(DartFileInput::new(
            "lib/platform_single_line.dart",
            "import 'src/stub.dart' if (dart.library.io) 'src/io.dart' if (dart.library.js_interop) 'src/web.dart' show PlatformApi;",
        ));
        assert_eq!(single_line.imports[0].configurations.len(), 2);
        assert_eq!(
            single_line.imports[0].configurations[1].condition,
            "dart.library.js_interop"
        );
        assert_eq!(single_line.imports[0].configurations[1].uri, "src/web.dart");
        assert_eq!(single_line.imports[0].combinators[0].names, ["PlatformApi"]);
    }

    #[test]
    fn distinguishes_part_of_from_part_directives() {
        let analysis = analyze_file(DartFileInput::new(
            "lib/src/model.dart",
            "part of '../models.dart';\n",
        ));

        assert!(analysis.parts.is_empty());
        assert_eq!(
            analysis.part_of.as_ref().map(|part| part.library.as_str()),
            Some("../models.dart")
        );
        assert_eq!(
            analysis.part_of.as_ref().map(|part| part.kind),
            Some(DartPartOfKind::Uri)
        );
    }

    #[test]
    fn parses_library_name_and_named_part_of_directive() {
        let library = analyze_file(DartFileInput::new(
            "lib/models.dart",
            "library app.models;\npart 'src/model.dart';\n",
        ));
        let part = analyze_file(DartFileInput::new(
            "lib/src/model.dart",
            "part of app.models;\n",
        ));

        assert_eq!(
            library
                .library
                .as_ref()
                .and_then(|value| value.name.as_deref()),
            Some("app.models")
        );
        assert_eq!(
            part.part_of.as_ref().map(|value| value.kind),
            Some(DartPartOfKind::LibraryName)
        );
    }

    #[test]
    fn parses_pubspec_dependencies() {
        let source = r#"
name: demo_app
dependencies:
  flutter:
    sdk: flutter
  http: ^1.2.0
dev_dependencies:
  test: ^1.25.0
"#;

        let analysis = parse_pubspec(PubspecInput::new("pubspec.yaml", source));

        assert_eq!(analysis.package_name.as_deref(), Some("demo_app"));
        assert!(analysis.dependencies.iter().any(|dependency| {
            dependency.name == "http"
                && dependency.section == PubspecDependencySection::Dependencies
                && dependency.version_or_source.as_deref() == Some("^1.2.0")
        }));
        assert!(analysis.dependencies.iter().any(|dependency| {
            dependency.name == "test"
                && dependency.section == PubspecDependencySection::DevDependencies
        }));
    }

    #[test]
    fn analyzes_project_summary_from_files_and_pubspecs() {
        let dart = r#"
import 'package:flutter/widgets.dart';

class HomeScreen extends StatelessWidget {
}
"#;
        let pubspec = r#"
name: demo_app
dependencies:
  flutter:
    sdk: flutter
"#;

        let analysis = analyze_project(
            DartProjectInput::new(
                "D:\\apps\\demo_app",
                vec![DartFileInput::new("lib\\main.dart", dart)],
                vec![PubspecInput::new("pubspec.yaml", pubspec)],
            )
            .with_package_configs(vec![PackageConfigInput::new(
                ".dart_tool/package_config.json",
                r#"{"configVersion":2,"packages":[]}"#,
            )]),
        );

        assert_eq!(analysis.root, "D:/apps/demo_app");
        assert_eq!(analysis.summary.dart_files, 1);
        assert_eq!(analysis.summary.pubspecs, 1);
        assert_eq!(analysis.summary.package_configs, 1);
        assert_eq!(analysis.package_configs[0].config_version, Some(2));
        assert_eq!(analysis.summary.imports, 1);
        assert_eq!(analysis.summary.flutter_widgets, 1);
        assert_eq!(analysis.summary.package_dependencies, 1);
        assert!(analysis.diagnostics.is_empty());
    }

    #[test]
    fn does_not_treat_indented_flutter_constructor_calls_as_declarations() {
        let source = r#"
import 'package:flutter/material.dart';

class HomeScreen extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Card(
      child: ListTile(
        title: Text('Home'),
      ),
    );
  }
}
"#;

        let analysis = analyze_file(DartFileInput::new("lib/home.dart", source));
        let names: Vec<_> = analysis
            .declarations
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect();

        assert!(names.contains(&"HomeScreen"));
        assert!(!names.contains(&"Card"));
        assert!(!names.contains(&"ListTile"));
        assert!(!names.contains(&"Text"));
    }

    #[test]
    fn class_constructor_initializer_is_top_level_variable_not_function() {
        let source = r#"
const storefrontSurfaceRegistry = StorefrontSurfaceRegistry(
  generated.generatedMobileManifest,
);

class StorefrontSurfaceRegistry {
}
"#;

        let analysis = analyze_file(DartFileInput::new(
            "lib/registry/storefront_surface_registry.dart",
            source,
        ));

        assert!(analysis.declarations.iter().any(|declaration| {
            declaration.name == "storefrontSurfaceRegistry"
                && declaration.kind == DartDeclarationKind::Variable
        }));
        assert_eq!(
            analysis
                .declarations
                .iter()
                .filter(|declaration| declaration.name == "StorefrontSurfaceRegistry")
                .count(),
            1
        );
    }

    #[test]
    fn treats_riverpod_consumer_widget_as_flutter_widget_hint() {
        let source = r#"
import 'package:flutter_riverpod/flutter_riverpod.dart';

class StorefrontHomePage extends ConsumerWidget {
}
"#;

        let analysis = analyze_file(DartFileInput::new("lib/routes/home.dart", source));

        assert!(analysis.flutter.widgets.iter().any(|widget| {
            widget.class_name == "StorefrontHomePage" && widget.base_class == "ConsumerWidget"
        }));
        assert!(analysis.flutter.imports_flutter);
    }

    #[test]
    fn extracts_go_route_hints_without_building_a_full_route_graph() {
        let source = r#"
import 'package:go_router/go_router.dart';

const homePath = '/';
const modulesRootPath = '/modules';
const String profilePath = '/profile';

GoRouter buildRouter() {
  return GoRouter(
    routes: [
      GoRoute(
        path: homePath,
        builder: (context, state) => const HomePage(),
      ),
      GoRoute(
        path: '$modulesRootPath/:routeSegment',
        name: 'modules:surface',
        builder: (context, state) => const ModulePage(),
      ),
      GoRoute(
        path: profilePath,
        builder: (context, state) => const ProfilePage(),
      ),
    ],
  );
}
"#;

        let analysis = analyze_file(DartFileInput::new("lib/routes/app_router.dart", source));

        assert_eq!(analysis.flutter.routes.len(), 3);
        assert!(analysis.string_constants.iter().any(|constant| {
            constant.name == "modulesRootPath" && constant.value == "/modules"
        }));
        assert!(analysis.flutter.routes.iter().any(|route| {
            route.path == "homePath"
                && route.path_kind == FlutterRoutePathKind::Expression
                && route.resolved_path.as_deref() == Some("/")
                && route.confidence == Confidence::High
        }));
        assert!(analysis.flutter.routes.iter().any(|route| {
            route.path == "$modulesRootPath/:routeSegment"
                && route.name.as_deref() == Some("modules:surface")
                && route.path_kind == FlutterRoutePathKind::Expression
                && route.resolved_path.as_deref() == Some("/modules/:routeSegment")
                && route.confidence == Confidence::High
        }));
        assert!(analysis.flutter.routes.iter().any(|route| {
            route.path == "profilePath"
                && route.resolved_path.as_deref() == Some("/profile")
                && route.confidence == Confidence::High
        }));
    }

    #[test]
    fn extracts_material_app_routes_map_hints() {
        let source = r#"
import 'package:flutter/material.dart';

class App extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      routes: <String, WidgetBuilder>{
        '/': (context) => const HomePage(),
        '/settings': (context) => const SettingsPage(),
      },
    );
  }
}
"#;

        let analysis = analyze_file(DartFileInput::new("lib/main.dart", source));

        assert_eq!(analysis.flutter.routes.len(), 2);
        assert!(analysis.flutter.routes.iter().any(|route| {
            route.constructor == "MaterialApp.routes"
                && route.path == "/"
                && route.path_kind == FlutterRoutePathKind::Literal
                && route.resolved_path.as_deref() == Some("/")
                && route.confidence == Confidence::High
        }));
        assert!(analysis.flutter.routes.iter().any(|route| {
            route.constructor == "MaterialApp.routes"
                && route.path == "/settings"
                && route.resolved_path.as_deref() == Some("/settings")
        }));
    }

    #[test]
    fn extracts_graphql_operations_from_raw_string_constants() {
        let source = r#"
const storefrontMobileCatalogQuery = r'''
  query StorefrontMobileCatalog($input: SearchPreviewInput!) {
    storefrontSearch(input: $input) {
      items {
        id
        title
      }
    }
  }
''';

const storefrontMobileCreateCartMutation = r'''
  mutation StorefrontMobileCreateCart($input: CreateStorefrontCartInput!) {
    createStorefrontCart(input: $input) {
      cart {
        id
      }
    }
  }
''';
"#;

        let analysis = analyze_file(DartFileInput::new(
            "lib/data/storefront_catalog_repository.dart",
            source,
        ));

        assert_eq!(analysis.graphql_operations.len(), 2);
        assert!(analysis.graphql_operations.iter().any(|operation| {
            operation.constant_name == "storefrontMobileCatalogQuery"
                && operation.operation_type == DartGraphqlOperationType::Query
                && operation.operation_name.as_deref() == Some("StorefrontMobileCatalog")
                && operation.variable_names == ["input"]
                && operation.root_fields == ["storefrontSearch"]
        }));
        assert!(analysis.graphql_operations.iter().any(|operation| {
            operation.constant_name == "storefrontMobileCreateCartMutation"
                && operation.operation_type == DartGraphqlOperationType::Mutation
                && operation.operation_name.as_deref() == Some("StorefrontMobileCreateCart")
                && operation.variable_names == ["input"]
                && operation.root_fields == ["createStorefrontCart"]
        }));
    }

    #[test]
    fn links_graphql_operation_constants_to_repository_methods() {
        let source = r#"
const moduleRegistryQuery = r'''
  query ModuleRegistry {
    moduleRegistry {
      moduleSlug
    }
  }
''';

const toggleModuleMutation = r'''
  mutation ToggleModule($moduleSlug: String!, $enabled: Boolean!) {
    toggleModule(moduleSlug: $moduleSlug, enabled: $enabled) {
      moduleSlug
    }
  }
''';

class GraphQlModulesRepository {
  Future<List<Object>> listModules() async {
    final result = await _client.query(
      QueryOptions(
        document: gql(moduleRegistryQuery),
      ),
    );
    return const <Object>[];
  }

  Future<Object> toggleModule() async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(toggleModuleMutation),
        variables: <String, dynamic>{
          'moduleSlug': moduleSlug,
          'enabled': enabled,
        },
      ),
    );
    return Object();
  }

  Future<Object> compensateModule(
    String moduleSlug,
  ) async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(compensateModuleMutation),
        variables: <String, dynamic>{'operationId': operationId},
      ),
    );
    return Object();
  }

  Future<Object> createCart() async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(createCartMutation),
        variables: <String, dynamic>{
          'input': <String, dynamic>{
            'email': email,
            'locale': locale,
          },
        },
      ),
    );
    return Object();
  }
}

final inlineOptions = MutationOptions(
  document: gql(r'''
    mutation InlineRefresh {
      refreshToken {
        accessToken
      }
    }
  '''),
);
"#;

        let analysis = analyze_file(DartFileInput::new(
            "lib/src/modules_repository.dart",
            source,
        ));

        assert_eq!(analysis.graphql_operation_uses.len(), 4);
        assert!(analysis.graphql_operation_uses.iter().any(|usage| {
            usage.constant_name == "moduleRegistryQuery"
                && usage.client_call == DartGraphqlClientCall::Query
                && usage.variable_names.is_empty()
                && usage.enclosing_callable.as_deref() == Some("listModules")
                && usage.enclosing_symbol.as_ref().is_some_and(|symbol| {
                    symbol.name == "listModules" && symbol.kind == DartEnclosingSymbolKind::Callable
                })
        }));
        assert!(analysis.graphql_operation_uses.iter().any(|usage| {
            usage.constant_name == "toggleModuleMutation"
                && usage.client_call == DartGraphqlClientCall::Mutation
                && usage.variable_names == ["enabled", "moduleSlug"]
                && usage.enclosing_callable.as_deref() == Some("toggleModule")
        }));
        assert!(analysis.graphql_operation_uses.iter().any(|usage| {
            usage.constant_name == "compensateModuleMutation"
                && usage.client_call == DartGraphqlClientCall::Mutation
                && usage.variable_names == ["operationId"]
                && usage.enclosing_callable.as_deref() == Some("compensateModule")
        }));
        assert!(analysis.graphql_operation_uses.iter().any(|usage| {
            usage.constant_name == "createCartMutation"
                && usage.client_call == DartGraphqlClientCall::Mutation
                && usage.variable_names == ["input"]
                && usage.enclosing_callable.as_deref() == Some("createCart")
        }));
    }

    #[test]
    fn links_graphql_operation_use_to_top_level_provider_initializer() {
        let source = r#"
const bootstrapProbeDocument = r'''
  query BootstrapProbe {
    me {
      id
    }
  }
''';

final authBootstrapProbeProvider = FutureProvider<BootstrapProbeResult>((
  ref,
) async {
  final result = await client.query(
    QueryOptions(
      document: gql(bootstrapProbeDocument),
    ),
  );
  return BootstrapProbeResult();
});
"#;

        let analysis = analyze_file(DartFileInput::new(
            "lib/app_shell/auth_bootstrap.dart",
            source,
        ));

        assert_eq!(analysis.graphql_operation_uses.len(), 1);
        let usage = &analysis.graphql_operation_uses[0];
        assert_eq!(usage.constant_name, "bootstrapProbeDocument");
        assert_eq!(usage.client_call, DartGraphqlClientCall::Query);
        assert!(usage.variable_names.is_empty());
        assert_eq!(usage.enclosing_callable, None);
        assert!(usage.enclosing_symbol.as_ref().is_some_and(|symbol| {
            symbol.name == "authBootstrapProbeProvider"
                && symbol.kind == DartEnclosingSymbolKind::Variable
        }));
    }
}
