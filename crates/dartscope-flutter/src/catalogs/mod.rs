mod assets;
mod localizations;

use dartscope_core::{
    DartDiagnostic, DartProjectAnalysis, PubspecAnalysis, SourceSpan, normalize_path,
};
use serde::{Deserialize, Serialize};

use crate::FlutterInventory;

pub use assets::{
    FlutterAssetDeclarationEntry, FlutterAssetDeclarationKind, FlutterAssetDeclarationRef,
};
pub use localizations::{FlutterArbCatalog, FlutterArbMessage, FlutterL10nConfiguration};

/// In-memory `l10n.yaml` input. DartScope never reads it implicitly inside library APIs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterL10nInput {
    pub path: String,
    pub source: String,
}

impl FlutterL10nInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

/// In-memory Application Resource Bundle input.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterArbInput {
    pub path: String,
    pub source: String,
}

impl FlutterArbInput {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: normalize_path(path.into()),
            source: source.into(),
        }
    }
}

/// Explicit supplemental inputs used for asset and localization catalog validation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FlutterCatalogInput {
    #[serde(default)]
    pub l10n_files: Vec<FlutterL10nInput>,
    #[serde(default)]
    pub arb_files: Vec<FlutterArbInput>,
}

impl FlutterCatalogInput {
    pub fn new(l10n_files: Vec<FlutterL10nInput>, arb_files: Vec<FlutterArbInput>) -> Self {
        Self {
            l10n_files,
            arb_files,
        }
    }
}

/// Builds the ordinary Flutter inventory and enriches it with declaration/catalog links.
///
/// The caller supplies all non-Dart source text explicitly. This function performs no I/O and
/// does not invoke Flutter tooling.
pub fn extract_flutter_inventory_with_catalogs(
    project: &DartProjectAnalysis,
    input: &FlutterCatalogInput,
) -> FlutterInventory {
    let mut inventory = crate::extract_flutter_inventory(project);
    let packages = package_contexts(project);
    assets::link_asset_declarations(&packages, &mut inventory);
    localizations::link_localization_catalogs(project, input, &packages, &mut inventory);
    sort_catalog_output(&mut inventory);
    inventory
}

#[derive(Debug, Clone)]
pub(crate) struct PackageContext<'a> {
    pub(crate) root: String,
    pub(crate) pubspec: &'a PubspecAnalysis,
}

pub(crate) fn package_contexts(project: &DartProjectAnalysis) -> Vec<PackageContext<'_>> {
    let mut packages = project
        .pubspecs
        .iter()
        .map(|pubspec| PackageContext {
            root: parent_path(&pubspec.path).to_string(),
            pubspec,
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        right
            .root
            .len()
            .cmp(&left.root.len())
            .then_with(|| left.pubspec.path.cmp(&right.pubspec.path))
    });
    packages
}

pub(crate) fn package_for_path<'a>(
    packages: &'a [PackageContext<'a>],
    path: &str,
) -> Option<&'a PackageContext<'a>> {
    packages
        .iter()
        .find(|package| path_is_within(path, &package.root))
}

pub(crate) fn path_is_within(path: &str, root: &str) -> bool {
    root.is_empty()
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|remaining| remaining.starts_with('/'))
}

pub(crate) fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub(crate) fn join_path(parts: &[&str]) -> String {
    let mut result = String::new();
    for part in parts {
        let trimmed = part.trim_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(trimmed);
    }
    result
}

pub(crate) fn normalized_catalog_path(path: &str) -> String {
    let replaced = path.replace('\\', "/");
    let mut output = Vec::new();
    for segment in replaced.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                output.pop();
            }
            other => output.push(other),
        }
    }
    output.join("/")
}

pub(crate) fn span_for_substring(source: &str, needle: &str) -> Option<SourceSpan> {
    let byte_start = source.find(needle)?;
    let byte_end = byte_start + needle.len();
    let prefix = &source[..byte_start];
    let start_line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let start_column = source[line_start..byte_start].chars().count() + 1;
    Some(SourceSpan {
        byte_start,
        byte_end,
        start_line,
        start_column,
        end_line: start_line,
        end_column: start_column + needle.chars().count(),
    })
}

fn sort_catalog_output(inventory: &mut FlutterInventory) {
    inventory.asset_declarations.sort_by(|left, right| {
        (&left.pubspec_path, &left.path).cmp(&(&right.pubspec_path, &right.path))
    });
    inventory.l10n_configurations.sort_by(|left, right| {
        (&left.package_root, &left.path).cmp(&(&right.package_root, &right.path))
    });
    inventory
        .arb_catalogs
        .sort_by(|left, right| left.path.cmp(&right.path));
    inventory.diagnostics.sort_by(|left, right| {
        (
            left.path.as_deref().unwrap_or(""),
            left.span
                .as_ref()
                .map_or(usize::MAX, |span| span.byte_start),
            &left.code,
        )
            .cmp(&(
                right.path.as_deref().unwrap_or(""),
                right
                    .span
                    .as_ref()
                    .map_or(usize::MAX, |span| span.byte_start),
                &right.code,
            ))
    });
}

pub(crate) fn push_diagnostic(inventory: &mut FlutterInventory, diagnostic: DartDiagnostic) {
    inventory.diagnostics.push(diagnostic);
}

#[cfg(test)]
mod tests {
    use dartscope_core::{
        Confidence, DartFileInput, DartProjectInput, FlutterLocalizationSource, PubspecInput,
    };
    use dartscope_parse::analyze_project;

    use super::*;

    fn analyzed_project(pubspec: &str, dart: &str) -> DartProjectAnalysis {
        analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new("lib/main.dart", dart)],
            vec![PubspecInput::new("pubspec.yaml", pubspec)],
        ))
    }

    #[test]
    fn links_file_and_direct_directory_assets_without_claiming_nested_files() {
        let project = analyzed_project(
            concat!(
                "name: demo\n",
                "flutter:\n",
                "  assets:\n",
                "    - path: assets/logo.png\n",
                "      flavors: [development, production]\n",
                "      platforms: [android, web]\n",
                "    - assets/icons/\n",
                "    - assets/unused.json\n",
            ),
            concat!(
                "void build() {\n",
                "  Image.asset('assets/logo.png');\n",
                "  AssetImage('assets/icons/add.png');\n",
                "  Image.asset('assets/icons/nested/deep.png');\n",
                "  Image.asset('assets/missing.png');\n",
                "  Image.asset('../assets/logo.png');\n",
                "  Image.asset('icons/external.png', package: 'external_icons');\n",
                "  Image.asset('icons/dynamic.png', package: targetPackage);\n",
                "}\n",
            ),
        );

        let inventory =
            extract_flutter_inventory_with_catalogs(&project, &FlutterCatalogInput::default());

        assert_eq!(inventory.asset_declarations.len(), 3);
        assert_eq!(
            inventory
                .assets
                .iter()
                .filter(|asset| asset.declaration.is_some())
                .count(),
            2
        );
        let directory = inventory
            .asset_declarations
            .iter()
            .find(|declaration| declaration.path == "assets/icons/")
            .expect("directory declaration");
        assert_eq!(directory.kind, FlutterAssetDeclarationKind::Directory);
        assert_eq!(directory.use_count, 1);
        let logo = inventory
            .assets
            .iter()
            .find(|asset| asset.asset_path == "assets/logo.png")
            .and_then(|asset| asset.declaration.as_ref())
            .expect("linked logo declaration");
        assert_eq!(logo.flavors.as_slice(), ["development", "production"]);
        assert_eq!(logo.platforms.as_slice(), ["android", "web"]);

        let undeclared = inventory
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "flutter_asset_used_but_undeclared")
            .collect::<Vec<_>>();
        assert_eq!(undeclared.len(), 3);
        assert!(
            undeclared
                .iter()
                .all(|diagnostic| diagnostic.confidence == Some(Confidence::High))
        );
        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_asset_declared_but_unused"
                && diagnostic.message.contains("assets/unused.json")
                && diagnostic.confidence == Some(Confidence::Medium)
        }));
        assert!(
            !inventory
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("external.png") })
        );
        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_asset_package_unresolved"
                && diagnostic.message.contains("dynamic.png")
                && diagnostic.confidence == Some(Confidence::Medium)
        }));
        assert!(
            !undeclared
                .iter()
                .any(|diagnostic| diagnostic.message.contains("dynamic.png"))
        );
    }

    #[test]
    fn links_assets_to_the_nearest_nested_package_pubspec() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "packages/ui/lib/widget.dart",
                "void build() { Image.asset('assets/logo.png'); }\n",
            )],
            vec![
                PubspecInput::new(
                    "pubspec.yaml",
                    "name: root\nflutter:\n  assets:\n    - assets/logo.png\n",
                ),
                PubspecInput::new(
                    "packages/ui/pubspec.yaml",
                    "name: ui\nflutter:\n  assets:\n    - assets/logo.png\n",
                ),
            ],
        ));

        let inventory =
            extract_flutter_inventory_with_catalogs(&project, &FlutterCatalogInput::default());
        let declaration = inventory.assets[0]
            .declaration
            .as_ref()
            .expect("nested declaration link");
        assert_eq!(declaration.pubspec_path, "packages/ui/pubspec.yaml");
        assert!(
            !inventory
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "flutter_asset_used_but_undeclared" })
        );
    }

    #[test]
    fn parses_custom_l10n_and_links_generated_class_to_arb_keys() {
        let project = analyzed_project(
            "name: demo\nflutter:\n  generate: true\n",
            concat!(
                "void build(context) {\n",
                "  Strings.of(context).hello;\n",
                "  Strings.of(context).missing;\n",
                "  Strings.of(context).countLabel(2);\n",
                "  AppLocalizations.of(context).hello;\n",
                "}\n",
            ),
        );
        let input = FlutterCatalogInput::new(
            vec![FlutterL10nInput::new(
                "l10n.yaml",
                concat!(
                    "arb-dir: lib/i18n\n",
                    "template-arb-file: messages_en.arb\n",
                    "output-localization-file: strings.dart\n",
                    "output-class: Strings\n",
                ),
            )],
            vec![
                FlutterArbInput::new(
                    "lib/i18n/messages_en.arb",
                    r#"{"@@locale":"en","hello":"Hello","@hello":{"description":"Greeting"},"countLabel":"Count: {count}","@countLabel":{"placeholders":{"count":{"type":"int"}}}}"#,
                ),
                FlutterArbInput::new(
                    "lib/i18n/messages_es.arb",
                    r#"{"@@locale":"es","hello":"Hola","countLabel":"Cantidad: {count}"}"#,
                ),
            ],
        );

        let inventory = extract_flutter_inventory_with_catalogs(&project, &input);

        assert_eq!(inventory.l10n_configurations.len(), 1);
        let configuration = &inventory.l10n_configurations[0];
        assert_eq!(configuration.output_class, "Strings");
        assert_eq!(configuration.template_arb_path, "lib/i18n/messages_en.arb");
        assert_eq!(inventory.arb_catalogs.len(), 2);
        assert_eq!(inventory.arb_catalogs[0].messages.len(), 2);
        assert_eq!(
            inventory.arb_catalogs[0]
                .messages
                .iter()
                .map(|message| message.key.as_str())
                .collect::<Vec<_>>(),
            ["countLabel", "hello"]
        );

        let hello = inventory
            .localizations
            .iter()
            .find(|entry| {
                entry.generated_class.as_deref() == Some("Strings") && entry.key == "hello"
            })
            .expect("custom localization use");
        assert_eq!(
            hello.source,
            FlutterLocalizationSource::GeneratedLocalizationsOf
        );
        assert_eq!(
            hello.catalog_paths,
            ["lib/i18n/messages_en.arb", "lib/i18n/messages_es.arb"]
        );
        let count_label = inventory
            .localizations
            .iter()
            .find(|entry| entry.key == "countLabel")
            .expect("placeholder localization method");
        assert_eq!(count_label.generated_class.as_deref(), Some("Strings"));
        assert_eq!(count_label.catalog_paths.len(), 2);
        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_localization_key_missing"
                && diagnostic.message.contains("missing")
                && diagnostic.confidence == Some(Confidence::High)
        }));
        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_localization_class_unresolved"
                && diagnostic.message.contains("AppLocalizations")
        }));
    }

    #[test]
    fn explicit_root_l10n_links_without_a_pubspec() {
        let project = analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/main.dart",
                "void build(context) { AppLocalizations.of(context).title; }\n",
            )],
            Vec::new(),
        ));
        let input = FlutterCatalogInput::new(
            vec![FlutterL10nInput::new("l10n.yaml", "")],
            vec![FlutterArbInput::new(
                "lib/l10n/app_en.arb",
                r#"{"title":"Title"}"#,
            )],
        );

        let inventory = extract_flutter_inventory_with_catalogs(&project, &input);

        assert_eq!(inventory.localizations.len(), 1);
        assert_eq!(
            inventory.localizations[0].catalog_paths,
            ["lib/l10n/app_en.arb"]
        );
        assert!(
            !inventory
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "flutter_localization_class_unresolved" })
        );
    }

    #[test]
    fn missing_l10n_configuration_is_medium_confidence() {
        let project = analyzed_project(
            "name: demo\n",
            "void build(context) { AppLocalizations.of(context).title; }\n",
        );

        let inventory =
            extract_flutter_inventory_with_catalogs(&project, &FlutterCatalogInput::default());
        let diagnostic = inventory
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "flutter_localization_class_unresolved")
            .expect("missing configuration diagnostic");
        assert_eq!(diagnostic.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn uses_documented_l10n_defaults_when_generate_is_enabled() {
        let project = analyzed_project(
            "name: demo\nflutter:\n  generate: true\n",
            "void build(context) { AppLocalizations.of(context).title; }\n",
        );
        let input = FlutterCatalogInput::new(
            Vec::new(),
            vec![FlutterArbInput::new(
                "lib/l10n/app_en.arb",
                r#"{"title":"Demo"}"#,
            )],
        );

        let inventory = extract_flutter_inventory_with_catalogs(&project, &input);

        assert_eq!(inventory.l10n_configurations.len(), 1);
        assert!(!inventory.l10n_configurations[0].explicit);
        assert_eq!(
            inventory.l10n_configurations[0].output_class,
            "AppLocalizations"
        );
        assert!(
            !inventory
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code.starts_with("flutter_localization_") })
        );
        assert_eq!(
            inventory.localizations[0].catalog_paths,
            ["lib/l10n/app_en.arb"]
        );
    }

    #[test]
    fn invalid_l10n_field_types_are_not_silently_defaulted() {
        let project = analyzed_project("name: demo\n", "void main() {}\n");
        let input = FlutterCatalogInput::new(
            vec![FlutterL10nInput::new(
                "l10n.yaml",
                "output-class: [Strings]\n",
            )],
            Vec::new(),
        );

        let inventory = extract_flutter_inventory_with_catalogs(&project, &input);
        let diagnostic = inventory
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "flutter_l10n_invalid_yaml")
            .expect("invalid field diagnostic");
        assert_eq!(diagnostic.path.as_deref(), Some("l10n.yaml"));
        assert!(diagnostic.span.is_some());
        assert!(diagnostic.message.contains("output-class"));
    }

    #[test]
    fn malformed_catalog_inputs_produce_path_attributed_diagnostics() {
        let project = analyzed_project("name: demo\n", "void main() {}\n");
        let input = FlutterCatalogInput::new(
            vec![FlutterL10nInput::new("l10n.yaml", "arb-dir: [broken\n")],
            vec![FlutterArbInput::new("lib/l10n/app_en.arb", "{broken")],
        );

        let inventory = extract_flutter_inventory_with_catalogs(&project, &input);

        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_l10n_invalid_yaml"
                && diagnostic.path.as_deref() == Some("l10n.yaml")
                && diagnostic.confidence == Some(Confidence::High)
        }));
        assert!(inventory.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "flutter_arb_invalid_json"
                && diagnostic.path.as_deref() == Some("lib/l10n/app_en.arb")
                && diagnostic.confidence == Some(Confidence::High)
        }));
    }
}
