use std::collections::HashMap;

use dartscope_core::{
    Confidence, FlutterAssetHint, FlutterAssetSource, FlutterLocalizationHint,
    FlutterLocalizationSource, FlutterRouteHint, FlutterRoutePathKind, SourceSpan,
};

use crate::declarations::{next_identifier, quoted_value, resolve_interpolated_string};

#[derive(Debug, Clone)]
pub(crate) struct PendingRouteHint {
    constructor: String,
    span: SourceSpan,
    path: Option<RoutePathValue>,
    name: Option<String>,
}

impl PendingRouteHint {
    pub(crate) fn observe_line(
        &mut self,
        code_trimmed: &str,
        source_trimmed: &str,
        string_constants: &HashMap<String, String>,
    ) {
        if self.path.is_none() {
            self.path = route_path_argument(source_trimmed, string_constants);
        }
        if self.name.is_none() {
            self.name = route_name_argument(source_trimmed);
        }
        let _ = code_trimmed;
    }

    pub(crate) fn finish(self) -> Option<FlutterRouteHint> {
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

pub(crate) fn pending_route_from_line(
    code_trimmed: &str,
    source_trimmed: &str,
    span: SourceSpan,
    string_constants: &HashMap<String, String>,
) -> Option<PendingRouteHint> {
    let constructor = if code_trimmed.starts_with("GoRoute(") {
        "GoRoute"
    } else {
        return None;
    };

    Some(PendingRouteHint {
        constructor: constructor.to_string(),
        span,
        path: route_path_argument(source_trimmed, string_constants),
        name: route_name_argument(source_trimmed),
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

pub(crate) fn route_constructor_is_complete(line: &str) -> bool {
    let Some(start) = line.find("GoRoute(") else {
        return false;
    };
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for ch in line[start..].chars() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
    }

    false
}

pub(crate) fn starts_material_routes_map(trimmed: &str) -> bool {
    trimmed.starts_with("routes:") && trimmed.contains('{')
}

pub(crate) fn material_route_from_line(
    code_trimmed: &str,
    source_trimmed: &str,
    span: SourceSpan,
) -> Option<FlutterRouteHint> {
    if !code_trimmed.contains(':') {
        return None;
    }
    let (key, _) = source_trimmed.split_once(':')?;
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

pub(crate) fn flutter_asset_from_line(
    code_trimmed: &str,
    source_trimmed: &str,
    span: SourceSpan,
) -> Option<FlutterAssetHint> {
    let (source, marker) = if code_trimmed.contains("Image.asset(") {
        (FlutterAssetSource::ImageAsset, "Image.asset(")
    } else if code_trimmed.contains("AssetImage(") {
        (FlutterAssetSource::AssetImage, "AssetImage(")
    } else if code_trimmed.contains("rootBundle.loadString(") {
        (
            FlutterAssetSource::RootBundleLoadString,
            "rootBundle.loadString(",
        )
    } else if code_trimmed.contains("DefaultAssetBundle.of(")
        && code_trimmed.contains(".loadString(")
    {
        (
            FlutterAssetSource::DefaultAssetBundleLoadString,
            ".loadString(",
        )
    } else {
        return None;
    };

    let value = value_after_marker(source_trimmed, marker).and_then(quoted_value)?;
    Some(FlutterAssetHint {
        path: value,
        source,
        confidence: Confidence::High,
        span,
    })
}

pub(crate) fn flutter_localization_from_line(
    trimmed: &str,
    span: SourceSpan,
) -> Option<FlutterLocalizationHint> {
    let marker = "AppLocalizations.of(";
    let index = trimmed.find(marker)?;
    let after_context = &trimmed[index + marker.len()..];
    let close = after_context.find(')')?;
    let after_call = after_context[close + 1..].trim_start();
    let after_nullability = after_call
        .strip_prefix('!')
        .or_else(|| after_call.strip_prefix('?'))
        .unwrap_or(after_call)
        .trim_start();
    let key = after_nullability.strip_prefix('.')?;
    let key = next_identifier(key)?;
    Some(FlutterLocalizationHint {
        key,
        source: FlutterLocalizationSource::AppLocalizationsOf,
        confidence: Confidence::High,
        span,
    })
}

fn value_after_marker<'a>(trimmed: &'a str, marker: &str) -> Option<&'a str> {
    let index = trimmed.find(marker)?;
    Some(&trimmed[index + marker.len()..])
}

pub(crate) fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

fn named_argument_value<'a>(trimmed: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}:");
    let index = trimmed.find(&marker)?;
    if index > 0
        && trimmed[..index]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let value = trimmed[index + marker.len()..].trim();
    (!value.is_empty()).then_some(value)
}

fn route_path_value(value: &str, string_constants: &HashMap<String, String>) -> RoutePathValue {
    let value = value.trim();
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

    let value = value.split([',', ')']).next().unwrap_or(value).trim();

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

pub(crate) fn should_finish_route_hint(trimmed: &str) -> bool {
    trimmed.starts_with("builder:")
        || trimmed.starts_with("pageBuilder:")
        || trimmed.starts_with("redirect:")
        || trimmed.starts_with("routes:")
        || trimmed == "),"
        || trimmed == ")"
}
