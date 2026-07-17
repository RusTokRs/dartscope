use dartscope_core::{Confidence, DartDiagnostic, SourceSpan};
use serde::{Deserialize, Serialize};

use crate::FlutterInventory;

use super::{PackageContext, package_for_path, push_diagnostic};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterAssetDeclarationKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterAssetDeclarationRef {
    pub pubspec_path: String,
    pub path: String,
    pub kind: FlutterAssetDeclarationKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flavors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterAssetDeclarationEntry {
    pub pubspec_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    pub path: String,
    pub kind: FlutterAssetDeclarationKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flavors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    pub span: SourceSpan,
    pub use_count: usize,
}

pub(crate) fn link_asset_declarations(
    packages: &[PackageContext<'_>],
    inventory: &mut FlutterInventory,
) {
    inventory.asset_declarations = collect_declarations(packages);

    for asset_index in 0..inventory.assets.len() {
        link_one_asset(packages, inventory, asset_index);
    }

    let unused = inventory
        .asset_declarations
        .iter()
        .filter(|declaration| declaration.use_count == 0)
        .map(|declaration| {
            DartDiagnostic::warning(
                "flutter_asset_declared_but_unused",
                format!(
                    "Flutter asset declaration {:?} has no direct literal use in the analyzed package",
                    declaration.path
                ),
                Some(declaration.span.clone()),
            )
            .with_path(declaration.pubspec_path.clone())
            .with_confidence(Confidence::Medium)
        })
        .collect::<Vec<_>>();
    inventory.diagnostics.extend(unused);
}

fn collect_declarations(packages: &[PackageContext<'_>]) -> Vec<FlutterAssetDeclarationEntry> {
    let mut declarations = Vec::new();
    for package in packages {
        let flutter = &package.pubspec.configuration.flutter;
        if flutter.asset_configurations.is_empty() {
            declarations.extend(flutter.assets.iter().map(|asset| {
                declaration_entry(
                    package,
                    &asset.path,
                    Vec::new(),
                    Vec::new(),
                    asset.span.clone(),
                )
            }));
        } else {
            declarations.extend(flutter.asset_configurations.iter().map(|asset| {
                declaration_entry(
                    package,
                    &asset.path,
                    asset.flavors.clone(),
                    asset.platforms.clone(),
                    asset.span.clone(),
                )
            }));
        }
    }
    declarations
}

fn declaration_entry(
    package: &PackageContext<'_>,
    path: &str,
    flavors: Vec<String>,
    platforms: Vec<String>,
    span: SourceSpan,
) -> FlutterAssetDeclarationEntry {
    let mut path = normalized_asset_path(path);
    let is_directory = path.ends_with('/');
    let kind = if is_directory {
        path = path.trim_end_matches('/').to_string();
        path.push('/');
        FlutterAssetDeclarationKind::Directory
    } else {
        FlutterAssetDeclarationKind::File
    };
    FlutterAssetDeclarationEntry {
        pubspec_path: package.pubspec.path.clone(),
        package_name: package.pubspec.package_name.clone(),
        path,
        kind,
        flavors,
        platforms,
        span,
        use_count: 0,
    }
}

fn link_one_asset(
    packages: &[PackageContext<'_>],
    inventory: &mut FlutterInventory,
    asset_index: usize,
) {
    let asset = &inventory.assets[asset_index];
    let Some(package) = package_for_path(packages, &asset.file_path) else {
        report_undeclared(inventory, asset_index, Confidence::Medium);
        return;
    };

    if let Some(expression) = asset.package_expression.as_deref() {
        push_diagnostic(
            inventory,
            DartDiagnostic::warning(
                "flutter_asset_package_unresolved",
                format!(
                    "Flutter asset {:?} uses a non-literal package expression {expression:?}; local declaration matching was skipped",
                    asset.asset_path
                ),
                Some(asset.span.clone()),
            )
            .with_path(asset.file_path.clone())
            .with_confidence(Confidence::Medium),
        );
        return;
    }

    if asset
        .package
        .as_deref()
        .is_some_and(|requested| package.pubspec.package_name.as_deref() != Some(requested))
    {
        return;
    }

    let normalized_use = normalized_asset_path(&asset.asset_path);
    let matching_index = inventory
        .asset_declarations
        .iter()
        .enumerate()
        .filter(|(_, declaration)| declaration.pubspec_path == package.pubspec.path)
        .filter(|(_, declaration)| declaration_matches(declaration, &normalized_use))
        .max_by_key(|(_, declaration)| match declaration.kind {
            FlutterAssetDeclarationKind::File => usize::MAX,
            FlutterAssetDeclarationKind::Directory => declaration.path.len(),
        })
        .map(|(index, _)| index);

    if let Some(declaration_index) = matching_index {
        let declaration = &mut inventory.asset_declarations[declaration_index];
        declaration.use_count += 1;
        inventory.assets[asset_index].declaration = Some(FlutterAssetDeclarationRef {
            pubspec_path: declaration.pubspec_path.clone(),
            path: declaration.path.clone(),
            kind: declaration.kind,
            flavors: declaration.flavors.clone(),
            platforms: declaration.platforms.clone(),
            span: declaration.span.clone(),
        });
    } else {
        report_undeclared(inventory, asset_index, Confidence::High);
    }
}

fn normalized_asset_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    normalized
}

fn declaration_matches(declaration: &FlutterAssetDeclarationEntry, asset_path: &str) -> bool {
    match declaration.kind {
        FlutterAssetDeclarationKind::File => declaration.path == asset_path,
        FlutterAssetDeclarationKind::Directory => asset_path
            .strip_prefix(&declaration.path)
            .is_some_and(|remaining| !remaining.is_empty() && !remaining.contains('/')),
    }
}

fn report_undeclared(inventory: &mut FlutterInventory, asset_index: usize, confidence: Confidence) {
    let asset = &inventory.assets[asset_index];
    push_diagnostic(
        inventory,
        DartDiagnostic::warning(
            "flutter_asset_used_but_undeclared",
            format!(
                "Flutter asset {:?} is used directly but is not declared by the nearest pubspec",
                asset.asset_path
            ),
            Some(asset.span.clone()),
        )
        .with_path(asset.file_path.clone())
        .with_confidence(confidence),
    );
}
