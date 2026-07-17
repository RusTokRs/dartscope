use dartscope_core::DiagnosticSeverity;
use serde::{Deserialize, Serialize};

/// Stable identifier for one DartScope lint rule.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DartLintRuleId {
    #[serde(rename = "dartscope.forbidden_import")]
    ForbiddenImport,
    #[serde(rename = "dartscope.layer_boundary")]
    LayerBoundary,
    #[serde(rename = "dartscope.naming_convention")]
    NamingConvention,
    #[serde(rename = "dartscope.unresolved_part")]
    UnresolvedPart,
    #[serde(rename = "dartscope.orphan_file")]
    OrphanFile,
}

impl DartLintRuleId {
    /// Every built-in rule in deterministic execution order.
    pub const ALL: [Self; 5] = [
        Self::ForbiddenImport,
        Self::LayerBoundary,
        Self::NamingConvention,
        Self::UnresolvedPart,
        Self::OrphanFile,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForbiddenImport => "dartscope.forbidden_import",
            Self::LayerBoundary => "dartscope.layer_boundary",
            Self::NamingConvention => "dartscope.naming_convention",
            Self::UnresolvedPart => "dartscope.unresolved_part",
            Self::OrphanFile => "dartscope.orphan_file",
        }
    }

    /// Stable SARIF-compatible short name.
    pub const fn short_name(self) -> &'static str {
        match self {
            Self::ForbiddenImport => "forbidden_import",
            Self::LayerBoundary => "layer_boundary",
            Self::NamingConvention => "naming_convention",
            Self::UnresolvedPart => "unresolved_part",
            Self::OrphanFile => "orphan_file",
        }
    }

    /// Human-readable rule title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::ForbiddenImport => "Forbidden import",
            Self::LayerBoundary => "Layer boundary",
            Self::NamingConvention => "Naming convention",
            Self::UnresolvedPart => "Unresolved part",
            Self::OrphanFile => "Orphan file",
        }
    }

    /// Stable rule description used by command-facing metadata.
    pub const fn description(self) -> &'static str {
        match self {
            Self::ForbiddenImport => "Reports imports matching configured forbidden URI patterns.",
            Self::LayerBoundary => {
                "Reports resolved internal imports that cross configured layer boundaries."
            }
            Self::NamingConvention => {
                "Reports supported file and top-level declaration names outside configured conventions."
            }
            Self::UnresolvedPart => {
                "Reports part directives that do not resolve to a valid part file."
            }
            Self::OrphanFile => "Reports files unreachable from configured project entry points.",
        }
    }
}

/// Explicit rule severity override.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLintSeverityOverride {
    pub rule_id: DartLintRuleId,
    pub severity: DiagnosticSeverity,
}

/// How a forbidden import pattern is matched.
#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartImportPatternKind {
    Exact,
    #[default]
    Prefix,
}

/// One forbidden import URI pattern, optionally scoped to source paths.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartForbiddenImportPattern {
    pub uri: String,
    #[serde(default)]
    pub match_kind: DartImportPatternKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_prefix: Option<String>,
}

/// One source-layer rule denying resolved imports into selected target path prefixes.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLayerBoundary {
    pub source_prefix: String,
    #[serde(default)]
    pub denied_target_prefixes: Vec<String>,
}

/// Conservative naming checks over normalized file paths and top-level declarations.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartNamingRuleConfig {
    pub check_file_names: bool,
    pub check_top_level_declarations: bool,
    #[serde(default)]
    pub ignored_path_prefixes: Vec<String>,
}

impl Default for DartNamingRuleConfig {
    fn default() -> Self {
        Self {
            check_file_names: true,
            check_top_level_declarations: true,
            ignored_path_prefixes: Vec::new(),
        }
    }
}

/// Reachability roots and ignored paths for the orphan-file rule.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartOrphanFileRuleConfig {
    #[serde(default)]
    pub entry_points: Vec<String>,
    #[serde(default)]
    pub ignored_path_prefixes: Vec<String>,
}

/// Complete configuration for deterministic lint execution.
///
/// The default enables no rules. Callers opt in by populating `enabled_rules`.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartLintConfig {
    #[serde(default)]
    pub enabled_rules: Vec<DartLintRuleId>,
    #[serde(default)]
    pub severity_overrides: Vec<DartLintSeverityOverride>,
    #[serde(default)]
    pub forbidden_imports: Vec<DartForbiddenImportPattern>,
    #[serde(default)]
    pub layer_boundaries: Vec<DartLayerBoundary>,
    #[serde(default)]
    pub naming: DartNamingRuleConfig,
    #[serde(default)]
    pub orphan_files: DartOrphanFileRuleConfig,
}

impl DartLintConfig {
    pub fn new(enabled_rules: impl IntoIterator<Item = DartLintRuleId>) -> Self {
        Self {
            enabled_rules: enabled_rules.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn all_rules() -> Self {
        Self::new(DartLintRuleId::ALL)
    }

    pub(crate) fn enabled_rule_ids(&self) -> Vec<DartLintRuleId> {
        let mut enabled = self.enabled_rules.clone();
        enabled.sort();
        enabled.dedup();
        enabled
    }

    pub(crate) fn severity(&self, rule_id: DartLintRuleId) -> DiagnosticSeverity {
        self.severity_overrides
            .iter()
            .rev()
            .find(|rule| rule.rule_id == rule_id)
            .map(|rule| rule.severity)
            .unwrap_or(DiagnosticSeverity::Warning)
    }
}
