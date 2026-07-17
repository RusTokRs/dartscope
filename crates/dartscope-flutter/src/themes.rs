use dartscope_core::{
    Confidence, DartFileAnalysis, DartInvocation, DartInvocationArgument, DartProjectAnalysis,
    SourceSpan,
};
use serde::{Deserialize, Serialize};

/// Deterministic official Flutter theme facts derived from normalized invocation data.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterThemeFacts {
    /// Supported `ThemeData` construction sites.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constructions: Vec<FlutterThemeConstruction>,
    /// Supported application-level and subtree theme assignments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applications: Vec<FlutterThemeApplication>,
}

/// One supported `ThemeData` construction site.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterThemeConstruction {
    /// File path containing the construction.
    pub file_path: String,
    /// Normalized official constructor family.
    pub constructor: FlutterThemeConstructor,
    /// Raw `brightness` expression when explicitly supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<String>,
    /// Raw `colorScheme` expression when explicitly supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<String>,
    /// Raw `colorSchemeSeed` expression when explicitly supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme_seed: Option<String>,
    /// Raw `useMaterial3` expression when explicitly supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_material3: Option<String>,
    /// Confidence for exact official-constructor matching under a Material import.
    pub confidence: Confidence,
    /// Exact invocation span.
    pub span: SourceSpan,
}

/// Supported official `ThemeData` constructor families.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterThemeConstructor {
    ThemeData,
    ThemeDataLight,
    ThemeDataDark,
    ThemeDataFrom,
}

/// One supported assignment of a theme expression to an official Flutter theme slot.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterThemeApplication {
    /// File path containing the assignment.
    pub file_path: String,
    /// Normalized application slot.
    pub application: FlutterThemeApplicationKind,
    /// Original expression supplied to the slot.
    pub expression: String,
    /// Confidence for exact official-argument matching under a Material import.
    pub confidence: Confidence,
    /// Exact named-argument span.
    pub span: SourceSpan,
}

/// Supported official Material theme application slots.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterThemeApplicationKind {
    MaterialAppTheme,
    MaterialAppDarkTheme,
    MaterialAppHighContrastTheme,
    MaterialAppHighContrastDarkTheme,
    MaterialAppThemeMode,
    ThemeData,
    AnimatedThemeData,
}

/// Derives official Material theme facts from one parsed Dart file.
///
/// The function consumes only normalized parser facts. It does not evaluate widget trees,
/// resolve identifiers, or execute Flutter code.
pub fn derive_flutter_theme_facts(file: &DartFileAnalysis) -> FlutterThemeFacts {
    if !imports_material(file) {
        return FlutterThemeFacts::default();
    }

    let mut facts = FlutterThemeFacts::default();
    for invocation in &file.invocations {
        if let Some(construction) = theme_construction(file, invocation) {
            facts.constructions.push(construction);
        }
        facts
            .applications
            .extend(theme_applications(file, invocation));
    }
    sort_and_deduplicate(&mut facts);
    facts
}

/// Aggregates deterministic official Material theme facts across a parsed project.
pub fn extract_flutter_theme_facts(project: &DartProjectAnalysis) -> FlutterThemeFacts {
    let mut facts = FlutterThemeFacts::default();
    for file in &project.files {
        let file_facts = derive_flutter_theme_facts(file);
        facts.constructions.extend(file_facts.constructions);
        facts.applications.extend(file_facts.applications);
    }
    sort_and_deduplicate(&mut facts);
    facts
}

fn theme_construction(
    file: &DartFileAnalysis,
    invocation: &DartInvocation,
) -> Option<FlutterThemeConstruction> {
    let constructor = theme_constructor(&invocation.target)?;
    Some(FlutterThemeConstruction {
        file_path: file.path.clone(),
        constructor,
        brightness: named_expression(invocation, "brightness"),
        color_scheme: named_expression(invocation, "colorScheme"),
        color_scheme_seed: named_expression(invocation, "colorSchemeSeed"),
        use_material3: named_expression(invocation, "useMaterial3"),
        confidence: Confidence::High,
        span: invocation.span.clone(),
    })
}

fn theme_constructor(target: &str) -> Option<FlutterThemeConstructor> {
    let parts: Vec<_> = target.split('.').collect();
    match parts.as_slice() {
        [.., "ThemeData"] => Some(FlutterThemeConstructor::ThemeData),
        [.., "ThemeData", "light"] => Some(FlutterThemeConstructor::ThemeDataLight),
        [.., "ThemeData", "dark"] => Some(FlutterThemeConstructor::ThemeDataDark),
        [.., "ThemeData", "from"] => Some(FlutterThemeConstructor::ThemeDataFrom),
        _ => None,
    }
}

fn theme_applications(
    file: &DartFileAnalysis,
    invocation: &DartInvocation,
) -> Vec<FlutterThemeApplication> {
    if is_material_app_target(&invocation.target) {
        return material_applications(file, invocation);
    }

    let application = match invocation.target.rsplit('.').next() {
        Some("Theme") => FlutterThemeApplicationKind::ThemeData,
        Some("AnimatedTheme") => FlutterThemeApplicationKind::AnimatedThemeData,
        _ => return Vec::new(),
    };
    named_application(file, invocation, "data", application)
        .into_iter()
        .collect()
}

fn material_applications(
    file: &DartFileAnalysis,
    invocation: &DartInvocation,
) -> Vec<FlutterThemeApplication> {
    [
        ("theme", FlutterThemeApplicationKind::MaterialAppTheme),
        (
            "darkTheme",
            FlutterThemeApplicationKind::MaterialAppDarkTheme,
        ),
        (
            "highContrastTheme",
            FlutterThemeApplicationKind::MaterialAppHighContrastTheme,
        ),
        (
            "highContrastDarkTheme",
            FlutterThemeApplicationKind::MaterialAppHighContrastDarkTheme,
        ),
        (
            "themeMode",
            FlutterThemeApplicationKind::MaterialAppThemeMode,
        ),
    ]
    .into_iter()
    .filter_map(|(name, application)| named_application(file, invocation, name, application))
    .collect()
}

fn named_application(
    file: &DartFileAnalysis,
    invocation: &DartInvocation,
    name: &str,
    application: FlutterThemeApplicationKind,
) -> Option<FlutterThemeApplication> {
    let argument = named_argument(invocation, name)?;
    Some(FlutterThemeApplication {
        file_path: file.path.clone(),
        application,
        expression: argument.expression.trim().to_string(),
        confidence: Confidence::High,
        span: argument.span.clone(),
    })
}

fn named_expression(invocation: &DartInvocation, name: &str) -> Option<String> {
    named_argument(invocation, name).map(|argument| argument.expression.trim().to_string())
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

fn imports_material(file: &DartFileAnalysis) -> bool {
    file.imports
        .iter()
        .any(|import| import.uri == "package:flutter/material.dart")
}

fn is_material_app_target(target: &str) -> bool {
    let parts: Vec<_> = target.split('.').collect();
    matches!(
        parts.as_slice(),
        [.., "MaterialApp"] | [.., "MaterialApp", "router"]
    )
}

fn sort_and_deduplicate(facts: &mut FlutterThemeFacts) {
    facts.constructions.sort_by(|left, right| {
        (
            &left.file_path,
            left.span.byte_start,
            left.span.byte_end,
            left.constructor,
        )
            .cmp(&(
                &right.file_path,
                right.span.byte_start,
                right.span.byte_end,
                right.constructor,
            ))
    });
    facts.constructions.dedup();
    facts.applications.sort_by(|left, right| {
        (
            &left.file_path,
            left.span.byte_start,
            left.span.byte_end,
            left.application,
        )
            .cmp(&(
                &right.file_path,
                right.span.byte_start,
                right.span.byte_end,
                right.application,
            ))
    });
    facts.applications.dedup();
}
