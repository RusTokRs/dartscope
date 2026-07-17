use std::collections::{BTreeMap, BTreeSet};

use dartscope_core::{
    Confidence, DartDiagnostic, DartInvocation, DartProjectAnalysis, FlutterLocalizationSource,
    SourceSpan,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

use crate::{FlutterInventory, FlutterLocalizationEntry};

use super::{
    FlutterArbInput, FlutterCatalogInput, FlutterL10nInput, PackageContext, join_path,
    normalized_catalog_path, package_for_path, parent_path, push_diagnostic, span_for_substring,
};

const DEFAULT_ARB_DIR: &str = "lib/l10n";
const DEFAULT_TEMPLATE_ARB: &str = "app_en.arb";
const DEFAULT_OUTPUT_FILE: &str = "app_localizations.dart";
const DEFAULT_OUTPUT_CLASS: &str = "AppLocalizations";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterL10nConfiguration {
    pub path: String,
    pub package_root: String,
    pub explicit: bool,
    pub arb_dir: String,
    pub template_arb_file: String,
    pub template_arb_path: String,
    pub output_localization_file: String,
    pub output_localization_path: String,
    pub output_class: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterArbCatalog {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    pub messages: Vec<FlutterArbMessage>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterArbMessage {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
}

pub(crate) fn link_localization_catalogs(
    project: &DartProjectAnalysis,
    input: &FlutterCatalogInput,
    packages: &[PackageContext<'_>],
    inventory: &mut FlutterInventory,
) {
    let configurations = parse_configurations(input, packages, inventory);
    let catalogs = parse_catalogs(&input.arb_files, inventory);
    inventory.l10n_configurations = configurations.clone();
    inventory.arb_catalogs = catalogs.clone();

    let catalog_map = catalogs
        .iter()
        .map(|catalog| (catalog.path.as_str(), catalog))
        .collect::<BTreeMap<_, _>>();
    report_missing_templates(&configurations, &catalog_map, inventory);
    link_generated_localization_uses(project, &configurations, &catalogs, &catalog_map, inventory);
}

fn report_missing_templates(
    configurations: &[FlutterL10nConfiguration],
    catalog_map: &BTreeMap<&str, &FlutterArbCatalog>,
    inventory: &mut FlutterInventory,
) {
    for configuration in configurations {
        if catalog_map.contains_key(configuration.template_arb_path.as_str()) {
            continue;
        }
        push_diagnostic(
            inventory,
            DartDiagnostic::warning(
                "flutter_l10n_template_missing",
                format!(
                    "configured template ARB {:?} was not supplied",
                    configuration.template_arb_path
                ),
                None,
            )
            .with_path(configuration.path.clone())
            .with_confidence(if configuration.explicit {
                Confidence::High
            } else {
                Confidence::Medium
            }),
        );
    }
}

fn link_generated_localization_uses(
    project: &DartProjectAnalysis,
    configurations: &[FlutterL10nConfiguration],
    catalogs: &[FlutterArbCatalog],
    catalog_map: &BTreeMap<&str, &FlutterArbCatalog>,
    inventory: &mut FlutterInventory,
) {
    for file in &project.files {
        let configuration = configurations
            .iter()
            .filter(|configuration| super::path_is_within(&file.path, &configuration.package_root))
            .max_by_key(|configuration| configuration.package_root.len());
        for invocation in &file.invocations {
            link_generated_localization_use(
                &file.path,
                invocation,
                configuration,
                catalogs,
                catalog_map,
                inventory,
            );
        }
    }
}

fn link_generated_localization_use(
    file_path: &str,
    invocation: &DartInvocation,
    configuration: Option<&FlutterL10nConfiguration>,
    catalogs: &[FlutterArbCatalog],
    catalog_map: &BTreeMap<&str, &FlutterArbCatalog>,
    inventory: &mut FlutterInventory,
) {
    let Some((class_name, key)) = generated_localization_use(invocation) else {
        return;
    };
    let Some(configuration) = configuration else {
        if class_name == DEFAULT_OUTPUT_CLASS || class_name.ends_with("Localizations") {
            push_diagnostic(
                inventory,
                DartDiagnostic::warning(
                    "flutter_localization_class_unresolved",
                    format!(
                        "generated localization class {class_name:?} cannot be linked because no l10n configuration was supplied"
                    ),
                    Some(invocation.source_line_span.clone()),
                )
                .with_path(file_path.to_string())
                .with_confidence(Confidence::Medium),
            );
        }
        return;
    };

    if class_name != configuration.output_class {
        if class_name.ends_with("Localizations") {
            push_diagnostic(
                inventory,
                DartDiagnostic::warning(
                    "flutter_localization_class_unresolved",
                    format!(
                        "generated localization class {class_name:?} does not match configured output class {:?}",
                        configuration.output_class
                    ),
                    Some(invocation.source_line_span.clone()),
                )
                .with_path(file_path.to_string())
                .with_confidence(Confidence::High),
            );
        }
        return;
    }

    let Some(template) = catalog_map
        .get(configuration.template_arb_path.as_str())
        .copied()
    else {
        push_diagnostic(
            inventory,
            DartDiagnostic::warning(
                "flutter_localization_class_unresolved",
                format!(
                    "generated localization class {class_name:?} cannot be validated because template ARB {:?} is unavailable",
                    configuration.template_arb_path
                ),
                Some(invocation.source_line_span.clone()),
            )
            .with_path(file_path.to_string())
            .with_confidence(Confidence::Medium),
        );
        return;
    };

    if !template.messages.iter().any(|message| message.key == key) {
        push_diagnostic(
            inventory,
            DartDiagnostic::warning(
                "flutter_localization_key_missing",
                format!(
                    "localization key {key:?} is referenced but is absent from template ARB {:?}",
                    template.path
                ),
                Some(invocation.source_line_span.clone()),
            )
            .with_path(file_path.to_string())
            .with_confidence(Confidence::High),
        );
    }

    upsert_localization_entry(
        inventory,
        file_path,
        invocation,
        class_name,
        key,
        catalogs_for_key(catalogs, configuration, key),
    );
}

fn parse_configurations(
    input: &FlutterCatalogInput,
    packages: &[PackageContext<'_>],
    inventory: &mut FlutterInventory,
) -> Vec<FlutterL10nConfiguration> {
    let mut configurations = Vec::new();
    let mut configured_roots = BTreeSet::new();

    let mut l10n_files = input.l10n_files.iter().collect::<Vec<_>>();
    l10n_files.sort_by(|left, right| left.path.cmp(&right.path));
    for l10n in l10n_files {
        let package_root = package_for_path(packages, &l10n.path).map_or_else(
            || parent_path(&l10n.path).to_string(),
            |package| package.root.clone(),
        );
        if !configured_roots.insert(package_root.clone()) {
            push_diagnostic(
                inventory,
                DartDiagnostic::warning(
                    "flutter_l10n_duplicate_configuration",
                    "multiple l10n.yaml inputs resolve to the same package; only the first is used",
                    None,
                )
                .with_path(l10n.path.clone())
                .with_confidence(Confidence::High),
            );
            continue;
        }
        match parse_l10n_input(l10n, &package_root) {
            Ok(configuration) => configurations.push(configuration),
            Err(diagnostic) => push_diagnostic(inventory, *diagnostic),
        }
    }

    for package in packages {
        let generate = package.pubspec.configuration.flutter.generate_localizations == Some(true);
        let has_default_arb = input
            .arb_files
            .iter()
            .any(|arb| parent_path(&arb.path) == join_path(&[&package.root, DEFAULT_ARB_DIR]));
        if !configured_roots.contains(&package.root) && (generate || has_default_arb) {
            configurations.push(default_configuration(&package.root));
        }
    }

    configurations
}

fn parse_l10n_input(
    input: &FlutterL10nInput,
    package_root: &str,
) -> Result<FlutterL10nConfiguration, Box<DartDiagnostic>> {
    let documents = YamlLoader::load_from_str(&input.source).map_err(|error| {
        Box::new(
            DartDiagnostic::error(
                "flutter_l10n_invalid_yaml",
                format!("failed to parse l10n.yaml: {error}"),
                None,
            )
            .with_path(input.path.clone())
            .with_confidence(Confidence::High),
        )
    })?;
    let mapping = match documents.first() {
        None | Some(Yaml::Null) => None,
        Some(Yaml::Hash(mapping)) => Some(mapping),
        Some(_) => {
            return Err(Box::new(
                DartDiagnostic::error(
                    "flutter_l10n_invalid_yaml",
                    "l10n.yaml must contain a top-level mapping",
                    None,
                )
                .with_path(input.path.clone())
                .with_confidence(Confidence::High),
            ));
        }
    };

    let value = |key: &str, default: &str| -> Result<String, Box<DartDiagnostic>> {
        let Some(value) = mapping.and_then(|mapping| mapping.get(&Yaml::String(key.to_string())))
        else {
            return Ok(default.to_string());
        };
        value.as_str().map(ToString::to_string).ok_or_else(|| {
            Box::new(
                DartDiagnostic::error(
                    "flutter_l10n_invalid_yaml",
                    format!("l10n.yaml field {key:?} must be a string"),
                    span_for_substring(&input.source, key),
                )
                .with_path(input.path.clone())
                .with_confidence(Confidence::High),
            )
        })
    };
    let arb_dir = normalized_catalog_path(&value("arb-dir", DEFAULT_ARB_DIR)?);
    let template_arb_file = value("template-arb-file", DEFAULT_TEMPLATE_ARB)?;
    let output_localization_file = value("output-localization-file", DEFAULT_OUTPUT_FILE)?;
    let output_class = value("output-class", DEFAULT_OUTPUT_CLASS)?;
    let output_dir = normalized_catalog_path(&value("output-dir", &arb_dir)?);

    Ok(FlutterL10nConfiguration {
        path: input.path.clone(),
        package_root: package_root.to_string(),
        explicit: true,
        template_arb_path: join_path(&[package_root, &arb_dir, &template_arb_file]),
        output_localization_path: join_path(&[
            package_root,
            &output_dir,
            &output_localization_file,
        ]),
        arb_dir,
        template_arb_file,
        output_localization_file,
        output_class,
    })
}

fn default_configuration(package_root: &str) -> FlutterL10nConfiguration {
    FlutterL10nConfiguration {
        path: join_path(&[package_root, "l10n.yaml"]),
        package_root: package_root.to_string(),
        explicit: false,
        arb_dir: DEFAULT_ARB_DIR.to_string(),
        template_arb_file: DEFAULT_TEMPLATE_ARB.to_string(),
        template_arb_path: join_path(&[package_root, DEFAULT_ARB_DIR, DEFAULT_TEMPLATE_ARB]),
        output_localization_file: DEFAULT_OUTPUT_FILE.to_string(),
        output_localization_path: join_path(&[package_root, DEFAULT_ARB_DIR, DEFAULT_OUTPUT_FILE]),
        output_class: DEFAULT_OUTPUT_CLASS.to_string(),
    }
}

fn parse_catalogs(
    inputs: &[FlutterArbInput],
    inventory: &mut FlutterInventory,
) -> Vec<FlutterArbCatalog> {
    let mut catalogs = Vec::new();
    for input in inputs {
        match parse_arb_input(input) {
            Ok(catalog) => catalogs.push(catalog),
            Err(diagnostic) => push_diagnostic(inventory, *diagnostic),
        }
    }
    catalogs
}

fn parse_arb_input(input: &FlutterArbInput) -> Result<FlutterArbCatalog, Box<DartDiagnostic>> {
    let value: Value = serde_json::from_str(&input.source).map_err(|error| {
        Box::new(
            DartDiagnostic::error(
                "flutter_arb_invalid_json",
                format!("failed to parse ARB JSON: {error}"),
                None,
            )
            .with_path(input.path.clone())
            .with_confidence(Confidence::High),
        )
    })?;
    let Some(object) = value.as_object() else {
        return Err(Box::new(
            DartDiagnostic::error(
                "flutter_arb_invalid_json",
                "ARB file must contain a top-level JSON object",
                None,
            )
            .with_path(input.path.clone())
            .with_confidence(Confidence::High),
        ));
    };

    let locale = object
        .get("@@locale")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let mut messages = object
        .keys()
        .filter(|key| !key.starts_with('@'))
        .map(|key| FlutterArbMessage {
            key: key.clone(),
            span: serde_json::to_string(key)
                .ok()
                .and_then(|needle| span_for_substring(&input.source, &needle)),
        })
        .collect::<Vec<_>>();
    messages.sort_by(|left, right| left.key.cmp(&right.key));

    Ok(FlutterArbCatalog {
        path: input.path.clone(),
        locale,
        messages,
    })
}

fn generated_localization_use(invocation: &DartInvocation) -> Option<(&str, &str)> {
    if let Some(class_target) = invocation.target.strip_suffix(".of") {
        return Some((
            class_target.rsplit('.').next()?,
            invocation.result_members.first()?.as_str(),
        ));
    }

    let marker = invocation.target.rfind(".of.")?;
    let class_name = invocation.target[..marker].rsplit('.').next()?;
    let key = invocation.target[marker + ".of.".len()..]
        .split('.')
        .next()?;
    Some((class_name, key))
}

fn catalogs_for_key(
    catalogs: &[FlutterArbCatalog],
    configuration: &FlutterL10nConfiguration,
    key: &str,
) -> Vec<String> {
    let arb_root = join_path(&[&configuration.package_root, &configuration.arb_dir]);
    let mut paths = catalogs
        .iter()
        .filter(|catalog| parent_path(&catalog.path) == arb_root)
        .filter(|catalog| catalog.messages.iter().any(|message| message.key == key))
        .map(|catalog| catalog.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn upsert_localization_entry(
    inventory: &mut FlutterInventory,
    file_path: &str,
    invocation: &DartInvocation,
    class_name: &str,
    key: &str,
    catalog_paths: Vec<String>,
) {
    if let Some(entry) = inventory.localizations.iter_mut().find(|entry| {
        entry.file_path == file_path
            && entry.key == key
            && entry.span.byte_start == invocation.source_line_span.byte_start
    }) {
        entry.generated_class = Some(class_name.to_string());
        entry.catalog_paths = catalog_paths;
        return;
    }

    inventory.localizations.push(FlutterLocalizationEntry {
        file_path: file_path.to_string(),
        key: key.to_string(),
        source: FlutterLocalizationSource::GeneratedLocalizationsOf,
        generated_class: Some(class_name.to_string()),
        catalog_paths,
        confidence: Confidence::High,
        span: invocation.source_line_span.clone(),
    });
    inventory.localizations.sort_by(|left, right| {
        (&left.file_path, left.span.byte_start, &left.key).cmp(&(
            &right.file_path,
            right.span.byte_start,
            &right.key,
        ))
    });
    inventory.localizations.dedup();
}
