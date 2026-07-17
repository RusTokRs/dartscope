use std::collections::HashMap;

use dartscope_core::{
    Confidence, DartFileAnalysis, DartInvocation, DartInvocationArgument, DartProjectAnalysis,
    FlutterAssetHint, FlutterAssetSource, FlutterFileHints, FlutterLocalizationHint,
    FlutterLocalizationSource, FlutterRouteHint, FlutterRoutePathKind, FlutterWidgetHint,
};

/// Derives Flutter conventions from generic Dart declarations, imports, and invocations.
///
/// This function performs no source parsing and no I/O. The parser-independent facts in
/// [`DartFileAnalysis`] are the only input to the convention layer.
pub fn derive_flutter_file_hints(file: &DartFileAnalysis) -> FlutterFileHints {
    let constants: HashMap<_, _> = file
        .string_constants
        .iter()
        .map(|constant| (constant.name.as_str(), constant.value.as_str()))
        .collect();
    let mut hints = FlutterFileHints {
        imports_flutter: file
            .imports
            .iter()
            .any(|import| is_flutter_import(&import.uri)),
        ..FlutterFileHints::default()
    };

    for declaration in &file.declarations {
        if let Some(base_class) = declaration
            .extends
            .as_deref()
            .filter(|base| is_flutter_base(base))
        {
            hints.widgets.push(FlutterWidgetHint {
                class_name: declaration.name.clone(),
                base_class: base_class.to_string(),
                confidence: Confidence::High,
                span: declaration.span.clone(),
            });
        }
    }

    for invocation in &file.invocations {
        if let Some(asset) = asset_hint(invocation) {
            hints.assets.push(asset);
        }
        if let Some(localization) = localization_hint(invocation) {
            hints.localizations.push(localization);
        }
        hints.routes.extend(route_hints(
            invocation,
            &constants,
            imports_official_flutter(file),
        ));
    }

    sort_and_deduplicate(&mut hints);
    hints
}

/// Replaces the compatibility Flutter projection on one parsed file.
pub fn populate_flutter_file_hints(file: &mut DartFileAnalysis) {
    file.flutter = derive_flutter_file_hints(file);
}

/// Populates compatibility Flutter projections and summary counts on a parsed project.
pub fn populate_flutter_project_analysis(project: &mut DartProjectAnalysis) {
    for file in &mut project.files {
        populate_flutter_file_hints(file);
    }
    project.summary.flutter_widgets = project
        .files
        .iter()
        .map(|file| file.flutter.widgets.len())
        .sum();
    project.summary.flutter_routes = project
        .files
        .iter()
        .map(|file| file.flutter.routes.len())
        .sum();
    project.summary.flutter_assets = project
        .files
        .iter()
        .map(|file| file.flutter.assets.len())
        .sum();
    project.summary.flutter_localizations = project
        .files
        .iter()
        .map(|file| file.flutter.localizations.len())
        .sum();
}

pub(crate) fn effective_flutter_file_hints(file: &DartFileAnalysis) -> FlutterFileHints {
    let mut derived = derive_flutter_file_hints(file);
    if file.invocations.is_empty() {
        if derived.routes.is_empty() {
            derived.routes = file.flutter.routes.clone();
        }
        if derived.assets.is_empty() {
            derived.assets = file.flutter.assets.clone();
        }
        if derived.localizations.is_empty() {
            derived.localizations = file.flutter.localizations.clone();
        }
    }
    if derived.widgets.is_empty() {
        derived.widgets = file.flutter.widgets.clone();
    }
    derived.imports_flutter |= file.flutter.imports_flutter;
    sort_and_deduplicate(&mut derived);
    derived
}

fn asset_hint(invocation: &DartInvocation) -> Option<FlutterAssetHint> {
    let source = match invocation.target.as_str() {
        "Image.asset" => FlutterAssetSource::ImageAsset,
        "AssetImage" => FlutterAssetSource::AssetImage,
        "rootBundle.loadString" => FlutterAssetSource::RootBundleLoadString,
        "DefaultAssetBundle.of.loadString" => FlutterAssetSource::DefaultAssetBundleLoadString,
        _ => return None,
    };
    let path = positional_argument(invocation, 0)?.string_value.clone()?;
    let package_argument = named_argument(invocation, "package");
    Some(FlutterAssetHint {
        path,
        source,
        package: package_argument.and_then(|argument| argument.string_value.clone()),
        package_expression: package_argument
            .filter(|argument| argument.string_value.is_none())
            .map(|argument| argument.expression.clone()),
        confidence: Confidence::High,
        span: invocation.source_line_span.clone(),
    })
}

fn localization_hint(invocation: &DartInvocation) -> Option<FlutterLocalizationHint> {
    let key = if invocation.target == "AppLocalizations.of" {
        invocation.result_members.first()?.clone()
    } else if let Some(rest) = invocation.target.strip_prefix("AppLocalizations.of.") {
        rest.split('.').next_back()?.to_string()
    } else {
        return None;
    };
    Some(FlutterLocalizationHint {
        key,
        source: FlutterLocalizationSource::AppLocalizationsOf,
        confidence: Confidence::High,
        span: invocation.source_line_span.clone(),
    })
}

fn route_hints(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
    imports_official_flutter: bool,
) -> Vec<FlutterRouteHint> {
    if invocation.target == "GoRoute" {
        return go_route_hint(invocation, constants).into_iter().collect();
    }
    if !imports_official_flutter {
        return Vec::new();
    }
    if let Some(application) = official_application(&invocation.target) {
        return application_route_hints(invocation, constants, application);
    }
    navigator_route_hint(invocation, constants)
        .into_iter()
        .collect()
}

fn go_route_hint(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
) -> Option<FlutterRouteHint> {
    let path = named_argument(invocation, "path")?;
    let route_path = route_path_value(path, constants);
    Some(FlutterRouteHint {
        constructor: "GoRoute".to_string(),
        path: route_path.value,
        path_kind: route_path.kind,
        resolved_path: route_path.resolved,
        name: named_argument(invocation, "name").map(argument_display_value),
        confidence: route_path.confidence,
        span: invocation.source_line_span.clone(),
    })
}

fn official_application(target: &str) -> Option<&'static str> {
    match target.rsplit('.').next()? {
        "MaterialApp" => Some("MaterialApp"),
        "WidgetsApp" => Some("WidgetsApp"),
        "Navigator" => Some("Navigator"),
        _ => None,
    }
}

fn application_route_hints(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
    application: &str,
) -> Vec<FlutterRouteHint> {
    let mut hints = if application == "Navigator" {
        Vec::new()
    } else {
        route_table_hints(invocation, application)
    };
    if application != "Navigator"
        && let Some(home) = named_argument(invocation, "home")
    {
        hints.push(FlutterRouteHint {
            constructor: format!("{application}.home"),
            path: "/".to_string(),
            path_kind: FlutterRoutePathKind::Literal,
            resolved_path: Some("/".to_string()),
            name: None,
            confidence: Confidence::High,
            span: home.span.clone(),
        });
    }
    if let Some(initial_route) = named_argument(invocation, "initialRoute") {
        hints.push(route_hint_from_argument(
            format!("{application}.initialRoute"),
            initial_route,
            constants,
        ));
    }
    hints
}

fn route_table_hints(invocation: &DartInvocation, application: &str) -> Vec<FlutterRouteHint> {
    let Some(routes) = named_argument(invocation, "routes") else {
        return Vec::new();
    };
    routes
        .map_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.string_key.clone()?;
            Some(FlutterRouteHint {
                constructor: format!("{application}.routes"),
                path: path.clone(),
                path_kind: FlutterRoutePathKind::Literal,
                resolved_path: Some(path),
                name: None,
                confidence: Confidence::High,
                span: entry.source_line_span.clone(),
            })
        })
        .collect()
}

fn navigator_route_hint(
    invocation: &DartInvocation,
    constants: &HashMap<&str, &str>,
) -> Option<FlutterRouteHint> {
    let route_argument_index = navigator_route_argument_index(&invocation.target)?;
    let route = positional_argument(invocation, route_argument_index)?;
    Some(route_hint_from_argument(
        canonical_navigator_constructor(&invocation.target),
        route,
        constants,
    ))
}

fn navigator_route_argument_index(target: &str) -> Option<usize> {
    let method = target.rsplit('.').next()?;
    if !matches!(
        method,
        "pushNamed"
            | "pushReplacementNamed"
            | "pushNamedAndRemoveUntil"
            | "popAndPushNamed"
            | "restorablePushNamed"
            | "restorablePushReplacementNamed"
            | "restorablePushNamedAndRemoveUntil"
            | "restorablePopAndPushNamed"
    ) {
        return None;
    }
    let parts: Vec<_> = target.split('.').collect();
    let navigator = parts.iter().rposition(|part| *part == "Navigator")?;
    match parts.get(navigator + 1..) {
        Some([_, _]) if parts[navigator + 1] == "of" => Some(0),
        Some([_]) => Some(1),
        _ => None,
    }
}

fn canonical_navigator_constructor(target: &str) -> String {
    let method = target.rsplit('.').next().unwrap_or(target);
    if target.split('.').any(|part| part == "of") {
        format!("Navigator.of.{method}")
    } else {
        format!("Navigator.{method}")
    }
}

fn route_hint_from_argument(
    constructor: String,
    argument: &DartInvocationArgument,
    constants: &HashMap<&str, &str>,
) -> FlutterRouteHint {
    let route_path = route_path_value(argument, constants);
    FlutterRouteHint {
        constructor,
        path: route_path.value,
        path_kind: route_path.kind,
        resolved_path: route_path.resolved,
        name: None,
        confidence: route_path.confidence,
        span: argument.span.clone(),
    }
}

struct RoutePathValue {
    value: String,
    kind: FlutterRoutePathKind,
    resolved: Option<String>,
    confidence: Confidence,
}

fn route_path_value(
    argument: &DartInvocationArgument,
    constants: &HashMap<&str, &str>,
) -> RoutePathValue {
    if let Some(literal) = argument.string_value.as_deref() {
        let interpolated = literal.contains('$');
        let resolved = resolve_interpolated_string(literal, constants);
        return RoutePathValue {
            value: literal.to_string(),
            kind: if interpolated {
                FlutterRoutePathKind::Expression
            } else {
                FlutterRoutePathKind::Literal
            },
            confidence: if interpolated && resolved.is_none() {
                Confidence::Medium
            } else {
                Confidence::High
            },
            resolved,
        };
    }

    let value = argument.expression.trim().to_string();
    RoutePathValue {
        resolved: constants
            .get(value.as_str())
            .map(|value| (*value).to_string()),
        value,
        kind: FlutterRoutePathKind::Expression,
        confidence: Confidence::High,
    }
}

fn argument_display_value(argument: &DartInvocationArgument) -> String {
    argument
        .string_value
        .clone()
        .unwrap_or_else(|| argument.expression.trim().to_string())
}

fn positional_argument(
    invocation: &DartInvocation,
    index: usize,
) -> Option<&DartInvocationArgument> {
    invocation
        .arguments
        .iter()
        .filter(|argument| argument.name.is_none())
        .nth(index)
}

fn named_argument<'a>(
    invocation: &'a DartInvocation,
    name: &str,
) -> Option<&'a DartInvocationArgument> {
    invocation
        .arguments
        .iter()
        .find(|argument| argument.name.as_deref() == Some(name))
}

fn resolve_interpolated_string(value: &str, constants: &HashMap<&str, &str>) -> Option<String> {
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
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
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
        if braced && chars.next() != Some('}') {
            return None;
        }
        if name.is_empty() {
            return None;
        }
        resolved.push_str(constants.get(name.as_str())?);
    }
    Some(resolved)
}

fn is_flutter_base(base: &str) -> bool {
    let base = base.rsplit('.').next().unwrap_or(base);
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

fn imports_official_flutter(file: &DartFileAnalysis) -> bool {
    file.imports
        .iter()
        .any(|import| is_official_flutter_import(&import.uri))
}

fn is_official_flutter_import(uri: &str) -> bool {
    uri.starts_with("package:flutter/")
}

fn is_flutter_import(uri: &str) -> bool {
    is_official_flutter_import(uri) || uri.starts_with("package:flutter_riverpod/")
}

fn sort_and_deduplicate(hints: &mut FlutterFileHints) {
    hints.widgets.sort_by(|left, right| {
        (left.span.byte_start, &left.class_name).cmp(&(right.span.byte_start, &right.class_name))
    });
    hints.widgets.dedup();
    hints.routes.sort_by(|left, right| {
        (left.span.byte_start, &left.path).cmp(&(right.span.byte_start, &right.path))
    });
    hints.routes.dedup();
    hints.assets.sort_by(|left, right| {
        (left.span.byte_start, &left.path).cmp(&(right.span.byte_start, &right.path))
    });
    hints.assets.dedup();
    hints.localizations.sort_by(|left, right| {
        (left.span.byte_start, &left.key).cmp(&(right.span.byte_start, &right.key))
    });
    hints.localizations.dedup();
}
