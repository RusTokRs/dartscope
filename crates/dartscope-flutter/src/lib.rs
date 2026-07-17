//! Flutter convention analysis for DartScope.
//!
//! This crate derives Flutter conventions and project-level inventory on top of the
//! normalized [`dartscope_core::DartProjectAnalysis`] model. It does not parse Dart
//! source directly; all heuristics operate on generic imports, declarations, invocations,
//! constants, and compatibility projections from already-analyzed file results.
//!
//! # Design
//!
//! `dartscope-flutter` is intentionally optional for pure Dart consumers. It depends
//! only on `dartscope-core` and does not re-export parser internals.
//!
//! # Example
//!
//! ```rust,ignore
//! use dartscope_flutter::extract_flutter_inventory;
//!
//! let inventory = extract_flutter_inventory(&project_analysis);
//! println!("{} widgets found", inventory.widgets.len());
//! ```

mod conventions;

pub use conventions::{
    derive_flutter_file_hints, populate_flutter_file_hints, populate_flutter_project_analysis,
};

use dartscope_core::{
    Confidence, DartProjectAnalysis, FlutterAssetHint, FlutterLocalizationHint, FlutterRouteHint,
    FlutterRoutePathKind, FlutterWidgetHint, SourceSpan,
};

use crate::conventions::effective_flutter_file_hints;
use serde::{Deserialize, Serialize};

/// Project-level Flutter inventory aggregated from [`DartProjectAnalysis`].
///
/// Every field corresponds to findings detected across all Dart files in the project.
/// Confidence metadata is preserved from file-level hints so callers can filter by
/// certainty.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterInventory {
    /// All widget classes detected across the project (StatelessWidget, StatefulWidget,
    /// ConsumerWidget, etc.).
    pub widgets: Vec<FlutterWidgetEntry>,
    /// All route hints detected across the project (GoRoute, MaterialApp routes, etc.).
    pub routes: Vec<FlutterRouteEntry>,
    /// All direct asset references detected across the project.
    pub assets: Vec<FlutterAssetEntry>,
    /// All localization key references detected across the project.
    pub localizations: Vec<FlutterLocalizationEntry>,
    /// All Dart files that import `package:flutter/...`.
    pub flutter_file_paths: Vec<String>,
    /// Summary counts for quick inspection.
    pub summary: FlutterInventorySummary,
}

/// A widget class finding with its source location.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterWidgetEntry {
    /// File path (normalized, relative to project root).
    pub file_path: String,
    /// Name of the class that extends a Flutter widget base class.
    pub class_name: String,
    /// The base class detected (e.g. `StatelessWidget`, `ConsumerWidget`).
    pub base_class: String,
    /// Parser confidence for this finding.
    pub confidence: Confidence,
    /// Source location of the class declaration.
    pub span: SourceSpan,
}

/// A route hint finding with its source location.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterRouteEntry {
    /// File path (normalized, relative to project root).
    pub file_path: String,
    /// Route constructor or declaration kind (e.g. `GoRoute`, `MaterialApp routes`).
    pub constructor: String,
    /// The route path literal or expression.
    pub path: String,
    /// Whether `path` is a literal or an unresolved expression.
    pub path_kind: FlutterRoutePathKind,
    /// The resolved route path when the path was a same-file string constant reference.
    pub resolved_path: Option<String>,
    /// Optional route name.
    pub name: Option<String>,
    /// Parser confidence for this finding.
    pub confidence: Confidence,
    /// Source location.
    pub span: SourceSpan,
}

/// A direct asset reference with its source location.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterAssetEntry {
    /// File path (normalized, relative to project root).
    pub file_path: String,
    /// Asset path referenced in source.
    pub asset_path: String,
    /// The call site kind (e.g. `Image.asset`, `AssetImage`).
    pub source: dartscope_core::FlutterAssetSource,
    /// Parser confidence for this finding.
    pub confidence: Confidence,
    /// Source location.
    pub span: SourceSpan,
}

/// A localization key reference with its source location.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterLocalizationEntry {
    /// File path (normalized, relative to project root).
    pub file_path: String,
    /// The localization key referenced (e.g. `homeTitle`).
    pub key: String,
    /// The call site kind (e.g. `AppLocalizations.of`).
    pub source: dartscope_core::FlutterLocalizationSource,
    /// Parser confidence for this finding.
    pub confidence: Confidence,
    /// Source location.
    pub span: SourceSpan,
}

/// Summary counts for a [`FlutterInventory`].
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterInventorySummary {
    /// Total number of Flutter-importing files.
    pub flutter_files: usize,
    /// Total number of widget class findings.
    pub widgets: usize,
    /// Total number of route findings.
    pub routes: usize,
    /// Total number of asset findings.
    pub assets: usize,
    /// Total number of localization findings.
    pub localizations: usize,
}

/// Extract a project-level [`FlutterInventory`] from a [`DartProjectAnalysis`].
///
/// This function derives effective per-file Flutter hints from generic normalized facts
/// and aggregates them. It does not perform additional I/O or source parsing. Older payloads
/// without invocation facts fall back to their stored compatibility projection.
///
/// # Arguments
///
/// * `project` - A completed [`DartProjectAnalysis`] produced by
///   `dartscope_parse::analyze_project`.
///
/// # Returns
///
/// A [`FlutterInventory`] containing all findings indexed by file path.
pub fn extract_flutter_inventory(project: &DartProjectAnalysis) -> FlutterInventory {
    let mut widgets: Vec<FlutterWidgetEntry> = Vec::new();
    let mut routes: Vec<FlutterRouteEntry> = Vec::new();
    let mut assets: Vec<FlutterAssetEntry> = Vec::new();
    let mut localizations: Vec<FlutterLocalizationEntry> = Vec::new();
    let mut flutter_file_paths: Vec<String> = Vec::new();

    for file in &project.files {
        let hints = effective_flutter_file_hints(file);
        if hints.imports_flutter {
            flutter_file_paths.push(file.path.clone());
        }

        for widget in &hints.widgets {
            widgets.push(flutter_widget_entry(&file.path, widget));
        }

        for route in &hints.routes {
            routes.push(flutter_route_entry(&file.path, route));
        }

        for asset in &hints.assets {
            assets.push(flutter_asset_entry(&file.path, asset));
        }

        for localization in &hints.localizations {
            localizations.push(flutter_localization_entry(&file.path, localization));
        }
    }

    widgets.sort_by(|left, right| {
        (&left.file_path, left.span.byte_start, &left.class_name).cmp(&(
            &right.file_path,
            right.span.byte_start,
            &right.class_name,
        ))
    });
    routes.sort_by(|left, right| {
        (&left.file_path, left.span.byte_start, &left.path).cmp(&(
            &right.file_path,
            right.span.byte_start,
            &right.path,
        ))
    });
    assets.sort_by(|left, right| {
        (&left.file_path, left.span.byte_start, &left.asset_path).cmp(&(
            &right.file_path,
            right.span.byte_start,
            &right.asset_path,
        ))
    });
    localizations.sort_by(|left, right| {
        (&left.file_path, left.span.byte_start, &left.key).cmp(&(
            &right.file_path,
            right.span.byte_start,
            &right.key,
        ))
    });
    flutter_file_paths.sort();
    flutter_file_paths.dedup();

    let summary = FlutterInventorySummary {
        flutter_files: flutter_file_paths.len(),
        widgets: widgets.len(),
        routes: routes.len(),
        assets: assets.len(),
        localizations: localizations.len(),
    };

    FlutterInventory {
        widgets,
        routes,
        assets,
        localizations,
        flutter_file_paths,
        summary,
    }
}

fn flutter_widget_entry(file_path: &str, hint: &FlutterWidgetHint) -> FlutterWidgetEntry {
    FlutterWidgetEntry {
        file_path: file_path.to_string(),
        class_name: hint.class_name.clone(),
        base_class: hint.base_class.clone(),
        confidence: hint.confidence,
        span: hint.span.clone(),
    }
}

fn flutter_route_entry(file_path: &str, hint: &FlutterRouteHint) -> FlutterRouteEntry {
    FlutterRouteEntry {
        file_path: file_path.to_string(),
        constructor: hint.constructor.clone(),
        path: hint.path.clone(),
        path_kind: hint.path_kind,
        resolved_path: hint.resolved_path.clone(),
        name: hint.name.clone(),
        confidence: hint.confidence,
        span: hint.span.clone(),
    }
}

fn flutter_asset_entry(file_path: &str, hint: &FlutterAssetHint) -> FlutterAssetEntry {
    FlutterAssetEntry {
        file_path: file_path.to_string(),
        asset_path: hint.path.clone(),
        source: hint.source,
        confidence: hint.confidence,
        span: hint.span.clone(),
    }
}

fn flutter_localization_entry(
    file_path: &str,
    hint: &FlutterLocalizationHint,
) -> FlutterLocalizationEntry {
    FlutterLocalizationEntry {
        file_path: file_path.to_string(),
        key: hint.key.clone(),
        source: hint.source,
        confidence: hint.confidence,
        span: hint.span.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::{
        DartFileAnalysis, DartProjectAnalysis, DartProjectSummary, FlutterAssetSource,
        FlutterLocalizationSource, FlutterRoutePathKind,
    };

    fn empty_project() -> DartProjectAnalysis {
        DartProjectAnalysis {
            root: "test_project".to_string(),
            files: Vec::new(),
            pubspecs: Vec::new(),
            package_configs: Vec::new(),
            summary: DartProjectSummary::default(),
            diagnostics: Vec::new(),
        }
    }

    fn dummy_span() -> SourceSpan {
        SourceSpan {
            byte_start: 0,
            byte_end: 10,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 11,
        }
    }

    #[test]
    fn empty_project_yields_empty_inventory() {
        let project = empty_project();
        let inventory = extract_flutter_inventory(&project);
        assert!(inventory.widgets.is_empty());
        assert!(inventory.routes.is_empty());
        assert!(inventory.assets.is_empty());
        assert!(inventory.localizations.is_empty());
        assert!(inventory.flutter_file_paths.is_empty());
        assert_eq!(inventory.summary.widgets, 0);
        assert_eq!(inventory.summary.routes, 0);
    }

    #[test]
    fn inventory_aggregates_widgets_across_files() {
        let mut project = empty_project();

        let mut file = DartFileAnalysis::empty("lib/home.dart");
        file.flutter.imports_flutter = true;
        file.flutter.widgets.push(FlutterWidgetHint {
            class_name: "HomeScreen".to_string(),
            base_class: "StatelessWidget".to_string(),
            confidence: Confidence::High,
            span: dummy_span(),
        });

        let mut file2 = DartFileAnalysis::empty("lib/profile.dart");
        file2.flutter.imports_flutter = true;
        file2.flutter.widgets.push(FlutterWidgetHint {
            class_name: "ProfilePage".to_string(),
            base_class: "StatefulWidget".to_string(),
            confidence: Confidence::High,
            span: dummy_span(),
        });

        project.files = vec![file, file2];

        let inventory = extract_flutter_inventory(&project);
        assert_eq!(inventory.widgets.len(), 2);
        assert_eq!(inventory.flutter_file_paths.len(), 2);
        assert_eq!(inventory.summary.flutter_files, 2);
        assert_eq!(inventory.summary.widgets, 2);

        let home = inventory
            .widgets
            .iter()
            .find(|w| w.class_name == "HomeScreen")
            .unwrap();
        assert_eq!(home.file_path, "lib/home.dart");
        assert_eq!(home.base_class, "StatelessWidget");
    }

    #[test]
    fn inventory_aggregates_assets_and_localizations() {
        let mut project = empty_project();
        let mut file = DartFileAnalysis::empty("lib/main.dart");
        file.flutter.imports_flutter = true;
        file.flutter.assets.push(FlutterAssetHint {
            path: "assets/logo.png".to_string(),
            source: FlutterAssetSource::ImageAsset,
            confidence: Confidence::High,
            span: dummy_span(),
        });
        file.flutter.localizations.push(FlutterLocalizationHint {
            key: "appTitle".to_string(),
            source: FlutterLocalizationSource::AppLocalizationsOf,
            confidence: Confidence::High,
            span: dummy_span(),
        });
        project.files = vec![file];

        let inventory = extract_flutter_inventory(&project);
        assert_eq!(inventory.assets.len(), 1);
        assert_eq!(inventory.localizations.len(), 1);
        assert_eq!(inventory.assets[0].asset_path, "assets/logo.png");
        assert_eq!(inventory.localizations[0].key, "appTitle");
        assert_eq!(inventory.summary.assets, 1);
        assert_eq!(inventory.summary.localizations, 1);
    }

    #[test]
    fn inventory_aggregates_routes() {
        let mut project = empty_project();
        let mut file = DartFileAnalysis::empty("lib/router.dart");
        file.flutter.imports_flutter = true;
        file.flutter.routes.push(FlutterRouteHint {
            constructor: "GoRoute".to_string(),
            path: "/home".to_string(),
            path_kind: FlutterRoutePathKind::Literal,
            resolved_path: Some("/home".to_string()),
            name: Some("home".to_string()),
            confidence: Confidence::High,
            span: dummy_span(),
        });
        project.files = vec![file];

        let inventory = extract_flutter_inventory(&project);
        assert_eq!(inventory.routes.len(), 1);
        assert_eq!(inventory.routes[0].path, "/home");
        assert_eq!(inventory.routes[0].path_kind, FlutterRoutePathKind::Literal);
        assert_eq!(inventory.routes[0].name.as_deref(), Some("home"));
        assert_eq!(inventory.summary.routes, 1);
    }

    #[test]
    fn inventory_order_is_deterministic_for_unsorted_project_input() {
        let mut project = empty_project();
        let mut z_file = DartFileAnalysis::empty("lib/z.dart");
        z_file.flutter.imports_flutter = true;
        z_file.flutter.widgets.push(FlutterWidgetHint {
            class_name: "ZWidget".to_string(),
            base_class: "StatelessWidget".to_string(),
            confidence: Confidence::High,
            span: dummy_span(),
        });
        let mut a_file = DartFileAnalysis::empty("lib/a.dart");
        a_file.flutter.imports_flutter = true;
        a_file.flutter.widgets.push(FlutterWidgetHint {
            class_name: "AWidget".to_string(),
            base_class: "StatelessWidget".to_string(),
            confidence: Confidence::High,
            span: dummy_span(),
        });
        project.files = vec![z_file, a_file];

        let inventory = extract_flutter_inventory(&project);

        assert_eq!(inventory.flutter_file_paths, ["lib/a.dart", "lib/z.dart"]);
        assert_eq!(inventory.widgets[0].class_name, "AWidget");
        assert_eq!(inventory.widgets[1].class_name, "ZWidget");
    }

    #[test]
    fn non_flutter_files_are_not_counted_as_flutter_files() {
        let mut project = empty_project();
        let mut file = DartFileAnalysis::empty("lib/utils.dart");
        file.flutter.imports_flutter = false;
        project.files = vec![file];

        let inventory = extract_flutter_inventory(&project);
        assert!(inventory.flutter_file_paths.is_empty());
        assert_eq!(inventory.summary.flutter_files, 0);
    }
}
