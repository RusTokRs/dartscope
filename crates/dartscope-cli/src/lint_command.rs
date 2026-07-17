use std::collections::BTreeSet;
use std::fs;
use dartscope::{
    DartDiagnostic, DartForbiddenImportPattern, DartLayerBoundary,
    DartLintAnalysis, DartLintConfig, DartLintRuleId,
    DartLintSeverityOverride, DartNamingRuleConfig, DartOrphanFileRuleConfig,
    DartProjectAnalysis, DiagnosticSeverity, JsonContract, analyze_project, lint_project,
    to_json_contract_pretty,
};
use serde::Deserialize;

use super::{
    CliError, CliOutput, EXIT_FINDINGS, collect_project_input,
};

const CONFIG_VERSION: u16 = 1;
mod sarif;

pub(super) fn execute(path: &str, arguments: &[String]) -> Result<CliOutput, CliError> {
    let options = LintOptions::parse(arguments)?;
    let mut config = match options.config_path.as_deref() {
        Some(config_path) => read_config(config_path)?,
        None => LintFileConfig::default(),
    };
    if options.deny_warnings {
        config.failure_threshold = LintFailureThreshold::Warning;
    }

    let project = analyze_project(collect_project_input(path)?);
    if let Some(message) = malformed_project_message(&project) {
        return Err(CliError::project(message));
    }

    let engine_config = config.engine_config();
    let analysis = lint_project(&project, &engine_config);
    let exit_code = if config.failure_threshold.is_failure(&analysis) {
        EXIT_FINDINGS
    } else {
        0
    };
    let output = match options.format {
        LintOutputFormat::Json => to_json_contract_pretty(JsonContract::LintAnalysis, &analysis)
            .map_err(|error| {
                CliError::internal(format!("failed to serialize lint JSON output: {error}"))
            })?,
        LintOutputFormat::Sarif => {
            sarif::to_pretty_json(&analysis, &engine_config).map_err(|error| {
                CliError::internal(format!("failed to serialize SARIF output: {error}"))
            })?
        }
    };

    Ok(CliOutput::new(output, exit_code))
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum LintOutputFormat {
    #[default]
    Json,
    Sarif,
}

impl LintOutputFormat {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "json" => Ok(Self::Json),
            "sarif" => Ok(Self::Sarif),
            _ => Err(CliError::usage(format!(
                "invalid lint output format {value:?}; expected json or sarif"
            ))),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct LintOptions {
    config_path: Option<String>,
    format: LintOutputFormat,
    deny_warnings: bool,
}

impl LintOptions {
    fn parse(arguments: &[String]) -> Result<Self, CliError> {
        let mut options = Self::default();
        let mut format_seen = false;
        let mut index = 0;
        while index < arguments.len() {
            match arguments[index].as_str() {
                "--config" => {
                    if options.config_path.is_some() {
                        return Err(CliError::usage("--config may be specified only once"));
                    }
                    let value = arguments.get(index + 1).ok_or_else(|| {
                        CliError::usage("missing value for --config; expected a TOML file path")
                    })?;
                    options.config_path = Some(value.clone());
                    index += 2;
                }
                "--format" => {
                    if format_seen {
                        return Err(CliError::usage("--format may be specified only once"));
                    }
                    let value = arguments.get(index + 1).ok_or_else(|| {
                        CliError::usage("missing value for --format; expected json or sarif")
                    })?;
                    options.format = LintOutputFormat::parse(value)?;
                    format_seen = true;
                    index += 2;
                }
                "--deny-warnings" => {
                    if options.deny_warnings {
                        return Err(CliError::usage(
                            "--deny-warnings may be specified only once",
                        ));
                    }
                    options.deny_warnings = true;
                    index += 1;
                }
                argument => {
                    return Err(CliError::usage(format!(
                        "unexpected argument for lint: {argument}"
                    )));
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LintFailureThreshold {
    Never,
    Warning,
    #[default]
    Error,
}

impl LintFailureThreshold {
    fn is_failure(self, analysis: &DartLintAnalysis) -> bool {
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| match self {
                Self::Never => false,
                Self::Warning => matches!(
                    diagnostic.severity,
                    DiagnosticSeverity::Warning | DiagnosticSeverity::Error
                ),
                Self::Error => diagnostic.severity == DiagnosticSeverity::Error,
            })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LintFileConfig {
    version: u16,
    failure_threshold: LintFailureThreshold,
    enabled_rules: Vec<DartLintRuleId>,
    severity_overrides: Vec<DartLintSeverityOverride>,
    forbidden_imports: Vec<DartForbiddenImportPattern>,
    layer_boundaries: Vec<DartLayerBoundary>,
    naming: DartNamingRuleConfig,
    orphan_files: DartOrphanFileRuleConfig,
}

impl Default for LintFileConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            failure_threshold: LintFailureThreshold::Error,
            enabled_rules: Vec::new(),
            severity_overrides: Vec::new(),
            forbidden_imports: Vec::new(),
            layer_boundaries: Vec::new(),
            naming: DartNamingRuleConfig::default(),
            orphan_files: DartOrphanFileRuleConfig::default(),
        }
    }
}

impl LintFileConfig {
    fn validate_and_normalize(&mut self, path: &str) -> Result<(), CliError> {
        if self.version != CONFIG_VERSION {
            return Err(CliError::configuration(format!(
                "unsupported lint configuration version {} in {path}; expected {CONFIG_VERSION}",
                self.version
            )));
        }

        reject_duplicates(
            self.enabled_rules.iter().copied(),
            "enabled rule",
            path,
        )?;
        reject_duplicates(
            self.severity_overrides.iter().map(|override_| override_.rule_id),
            "severity override",
            path,
        )?;

        for pattern in &mut self.forbidden_imports {
            pattern.uri = pattern.uri.trim().to_string();
            if pattern.uri.is_empty() {
                return Err(CliError::configuration(format!(
                    "forbidden import URI cannot be empty in {path}"
                )));
            }
            normalize_optional_prefix(&mut pattern.source_prefix, "source_prefix", path)?;
        }
        for boundary in &mut self.layer_boundaries {
            boundary.source_prefix = normalize_required_prefix(
                &boundary.source_prefix,
                "layer source_prefix",
                path,
            )?;
            for target in &mut boundary.denied_target_prefixes {
                *target = normalize_required_prefix(target, "denied target prefix", path)?;
            }
        }
        normalize_prefixes(
            &mut self.naming.ignored_path_prefixes,
            "naming ignored path prefix",
            path,
        )?;
        normalize_prefixes(
            &mut self.orphan_files.entry_points,
            "orphan entry point",
            path,
        )?;
        normalize_prefixes(
            &mut self.orphan_files.ignored_path_prefixes,
            "orphan ignored path prefix",
            path,
        )?;
        Ok(())
    }

    fn engine_config(&self) -> DartLintConfig {
        DartLintConfig {
            enabled_rules: self.enabled_rules.clone(),
            severity_overrides: self.severity_overrides.clone(),
            forbidden_imports: self.forbidden_imports.clone(),
            layer_boundaries: self.layer_boundaries.clone(),
            naming: self.naming.clone(),
            orphan_files: self.orphan_files.clone(),
        }
    }
}

fn read_config(path: &str) -> Result<LintFileConfig, CliError> {
    let source = fs::read_to_string(path)
        .map_err(|error| CliError::input(format!("failed to read lint configuration {path}: {error}")))?;
    let mut config: LintFileConfig = toml::from_str(&source).map_err(|error| {
        CliError::configuration(format!("invalid lint configuration {path}: {error}"))
    })?;
    config.validate_and_normalize(path)?;
    Ok(config)
}

fn reject_duplicates(
    values: impl IntoIterator<Item = DartLintRuleId>,
    label: &str,
    path: &str,
) -> Result<(), CliError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(CliError::configuration(format!(
                "duplicate {label} {:?} in {path}",
                value.as_str()
            )));
        }
    }
    Ok(())
}

fn normalize_optional_prefix(
    value: &mut Option<String>,
    label: &str,
    path: &str,
) -> Result<(), CliError> {
    if let Some(prefix) = value {
        *prefix = normalize_required_prefix(prefix, label, path)?;
    }
    Ok(())
}

fn normalize_prefixes(
    values: &mut [String],
    label: &str,
    path: &str,
) -> Result<(), CliError> {
    for value in values {
        *value = normalize_required_prefix(value, label, path)?;
    }
    Ok(())
}

fn normalize_required_prefix(value: &str, label: &str, path: &str) -> Result<String, CliError> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err(CliError::configuration(format!(
            "{label} cannot be empty in {path}"
        )));
    }
    Ok(normalized)
}

fn malformed_project_message(project: &DartProjectAnalysis) -> Option<String> {
    for diagnostic in &project.diagnostics {
        if let Some(message) = project_error("<project>", diagnostic) {
            return Some(message);
        }
    }
    for file in &project.files {
        for diagnostic in &file.diagnostics {
            if let Some(message) = project_error(&file.path, diagnostic) {
                return Some(message);
            }
        }
    }
    for pubspec in &project.pubspecs {
        for diagnostic in &pubspec.diagnostics {
            if let Some(message) = project_error(&pubspec.path, diagnostic) {
                return Some(message);
            }
        }
    }
    for config in &project.package_configs {
        for diagnostic in &config.diagnostics {
            if let Some(message) = project_error(&config.path, diagnostic) {
                return Some(message);
            }
        }
    }
    None
}

fn project_error(fallback_path: &str, diagnostic: &DartDiagnostic) -> Option<String> {
    (diagnostic.severity == DiagnosticSeverity::Error).then(|| {
        let path = diagnostic.path.as_deref().unwrap_or(fallback_path);
        format!(
            "malformed project input at {path}: {}: {}",
            diagnostic.code, diagnostic.message
        )
    })
}

