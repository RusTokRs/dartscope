//! Source-derived Flutter hints: widgets, routes, assets, localizations.

use serde::{Deserialize, Serialize};

use crate::diagnostic::Confidence;
use crate::span::SourceSpan;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterFileHints {
    pub imports_flutter: bool,
    pub widgets: Vec<FlutterWidgetHint>,
    pub routes: Vec<FlutterRouteHint>,
    pub assets: Vec<FlutterAssetHint>,
    pub localizations: Vec<FlutterLocalizationHint>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterWidgetHint {
    pub class_name: String,
    pub base_class: String,
    pub confidence: Confidence,
    pub span: SourceSpan,
    /// The project class named in `extends` when the widget reaches `base_class` through other
    /// classes (`class Screen extends BaseScreen`, where `BaseScreen extends StatelessWidget`);
    /// absent for a direct subclass of `base_class`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherited_via: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterRouteHint {
    pub constructor: String,
    pub path: String,
    pub path_kind: FlutterRoutePathKind,
    pub resolved_path: Option<String>,
    pub name: Option<String>,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterAssetHint {
    pub path: String,
    pub source: FlutterAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Non-literal `package:` expression when exact package identity is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_expression: Option<String>,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterAssetSource {
    ImageAsset,
    AssetImage,
    RootBundleLoadString,
    DefaultAssetBundleLoadString,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterLocalizationHint {
    pub key: String,
    pub source: FlutterLocalizationSource,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterLocalizationSource {
    AppLocalizationsOf,
    GeneratedLocalizationsOf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterRoutePathKind {
    Literal,
    Expression,
}
