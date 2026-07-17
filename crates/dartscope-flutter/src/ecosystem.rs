use dartscope_core::pubspec::PubspecDependencySource;
use dartscope_core::{
    Confidence, DartFileAnalysis, DartProjectAnalysis, PubspecDependencySection, SourceSpan,
};
use serde::{Deserialize, Serialize};

/// Version of the opt-in Flutter ecosystem convention support table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FlutterEcosystemSupportTableVersion {
    #[default]
    V1,
}

impl FlutterEcosystemSupportTableVersion {
    /// Stable numeric support-table version.
    pub const fn version(self) -> u16 {
        match self {
            Self::V1 => 1,
        }
    }
}

/// Ecosystem conventions that callers may explicitly enable.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterEcosystemConvention {
    GoRouter,
    Provider,
    FlutterRiverpod,
    FlutterBloc,
}

/// Public, deterministic metadata describing the supported convention surface.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterEcosystemSupportTable {
    pub version: FlutterEcosystemSupportTableVersion,
    pub entries: Vec<FlutterEcosystemSupportEntry>,
}

/// One package entry in the support table.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterEcosystemSupportEntry {
    pub convention: FlutterEcosystemConvention,
    pub package: String,
    /// Dart pub constraint range covered by the current fixture contract.
    pub supported_range: String,
    /// Concrete package version used by the current normative fixture review.
    pub fixture_version: String,
    /// Canonical convention patterns emitted by this entry.
    pub patterns: Vec<String>,
}

/// Returns the current opt-in Flutter ecosystem support table.
pub fn flutter_ecosystem_support_table() -> FlutterEcosystemSupportTable {
    FlutterEcosystemSupportTable {
        version: FlutterEcosystemSupportTableVersion::V1,
        entries: vec![
            support_entry(
                FlutterEcosystemConvention::GoRouter,
                "go_router",
                ">=14.0.0 <18.0.0",
                "17.3.0",
                &[
                    "GoRouter",
                    "GoRoute",
                    "ShellRoute",
                    "StatefulShellRoute",
                    "StatefulShellBranch",
                ],
            ),
            support_entry(
                FlutterEcosystemConvention::Provider,
                "provider",
                ">=6.0.0 <7.0.0",
                "6.1.5+1",
                &[
                    "Provider",
                    "ChangeNotifierProvider",
                    "MultiProvider",
                    "ProxyProvider",
                    "Consumer",
                    "Selector",
                    "BuildContext.watch",
                    "BuildContext.read",
                    "BuildContext.select",
                ],
            ),
            support_entry(
                FlutterEcosystemConvention::FlutterRiverpod,
                "flutter_riverpod",
                ">=2.0.0 <4.0.0",
                "3.3.2",
                &[
                    "ProviderScope",
                    "Consumer",
                    "ConsumerWidget",
                    "ConsumerStatefulWidget",
                    "ConsumerState",
                ],
            ),
            support_entry(
                FlutterEcosystemConvention::FlutterBloc,
                "flutter_bloc",
                ">=8.0.0 <10.0.0",
                "9.1.1",
                &[
                    "BlocProvider",
                    "MultiBlocProvider",
                    "BlocBuilder",
                    "BlocListener",
                    "BlocConsumer",
                    "BlocSelector",
                    "RepositoryProvider",
                    "MultiRepositoryProvider",
                ],
            ),
        ],
    }
}

fn support_entry(
    convention: FlutterEcosystemConvention,
    package: &str,
    supported_range: &str,
    fixture_version: &str,
    patterns: &[&str],
) -> FlutterEcosystemSupportEntry {
    FlutterEcosystemSupportEntry {
        convention,
        package: package.to_string(),
        supported_range: supported_range.to_string(),
        fixture_version: fixture_version.to_string(),
        patterns: patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect(),
    }
}

/// Result of applying explicitly enabled ecosystem conventions to one project.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterEcosystemAnalysis {
    pub support_table_version: FlutterEcosystemSupportTableVersion,
    pub conventions: Vec<FlutterEcosystemConventionAnalysis>,
}

/// Version/evidence status for one enabled convention.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterEcosystemConventionStatus {
    Active,
    DependencyMissing,
    UnsupportedVersion,
    UnverifiableVersion,
}

/// Analysis for one explicitly enabled convention.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterEcosystemConventionAnalysis {
    pub convention: FlutterEcosystemConvention,
    pub package: String,
    pub supported_range: String,
    pub fixture_version: String,
    pub status: FlutterEcosystemConventionStatus,
    pub package_evidence: Vec<FlutterPackageEvidence>,
    pub findings: Vec<FlutterEcosystemFinding>,
}

/// Pubspec evidence used to activate or reject one convention entry.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterPackageEvidence {
    pub pubspec_path: String,
    pub dependency_section: PubspecDependencySection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PubspecDependencySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_constraint: Option<String>,
    pub span: SourceSpan,
}

/// One evidence-backed ecosystem convention occurrence.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlutterEcosystemFinding {
    pub file_path: String,
    pub pattern: String,
    pub kind: FlutterEcosystemFindingKind,
    pub confidence: Confidence,
    pub span: SourceSpan,
}

/// Source fact used for an ecosystem finding.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterEcosystemFindingKind {
    Invocation,
    BaseClass,
}

/// Applies only the ecosystem conventions explicitly listed in `enabled`.
///
/// A convention becomes active only when a project pubspec declares a supported version constraint.
/// Path, git, SDK, workspace, `any`, and otherwise unparseable constraints are retained as evidence
/// but do not activate package semantics.
pub fn analyze_flutter_ecosystem(
    project: &DartProjectAnalysis,
    enabled: &[FlutterEcosystemConvention],
) -> FlutterEcosystemAnalysis {
    let table = flutter_ecosystem_support_table();
    let mut enabled = enabled.to_vec();
    enabled.sort();
    enabled.dedup();

    let conventions = enabled
        .into_iter()
        .filter_map(|convention| {
            let entry = table
                .entries
                .iter()
                .find(|entry| entry.convention == convention)?;
            let package_evidence = package_evidence(project, &entry.package);
            let status = convention_status(entry, &package_evidence);
            let findings = if status == FlutterEcosystemConventionStatus::Active {
                convention_findings(project, convention, &entry.package)
            } else {
                Vec::new()
            };
            Some(FlutterEcosystemConventionAnalysis {
                convention,
                package: entry.package.clone(),
                supported_range: entry.supported_range.clone(),
                fixture_version: entry.fixture_version.clone(),
                status,
                package_evidence,
                findings,
            })
        })
        .collect();

    FlutterEcosystemAnalysis {
        support_table_version: table.version,
        conventions,
    }
}

fn package_evidence(project: &DartProjectAnalysis, package: &str) -> Vec<FlutterPackageEvidence> {
    let mut evidence = Vec::new();
    for pubspec in &project.pubspecs {
        for dependency in &pubspec.dependencies {
            if dependency.name != package {
                continue;
            }
            let source = dependency.structured_source();
            let version_constraint = source.as_ref().and_then(source_version_constraint);
            evidence.push(FlutterPackageEvidence {
                pubspec_path: pubspec.path.clone(),
                dependency_section: dependency.section,
                source,
                version_constraint,
                span: dependency.span.clone(),
            });
        }
    }
    evidence.sort_by(|left, right| {
        (
            &left.pubspec_path,
            dependency_section_order(left.dependency_section),
            left.span.byte_start,
            left.span.byte_end,
        )
            .cmp(&(
                &right.pubspec_path,
                dependency_section_order(right.dependency_section),
                right.span.byte_start,
                right.span.byte_end,
            ))
    });
    evidence.dedup();
    evidence
}

fn source_version_constraint(source: &PubspecDependencySource) -> Option<String> {
    match source {
        PubspecDependencySource::Version { constraint } => Some(constraint.clone()),
        PubspecDependencySource::Hosted {
            version: Some(version),
            ..
        } => Some(version.clone()),
        _ => None,
    }
}

fn dependency_section_order(section: PubspecDependencySection) -> u8 {
    match section {
        PubspecDependencySection::Dependencies => 0,
        PubspecDependencySection::DevDependencies => 1,
        PubspecDependencySection::DependencyOverrides => 2,
    }
}

fn convention_status(
    entry: &FlutterEcosystemSupportEntry,
    evidence: &[FlutterPackageEvidence],
) -> FlutterEcosystemConventionStatus {
    if evidence.is_empty() {
        return FlutterEcosystemConventionStatus::DependencyMissing;
    }
    let supported_majors = supported_majors(entry.convention);
    let mut saw_unverifiable = false;
    for evidence in evidence {
        let Some(constraint) = evidence.version_constraint.as_deref() else {
            saw_unverifiable = true;
            continue;
        };
        match constraint_intersects_supported_majors(constraint, supported_majors) {
            Some(true) => return FlutterEcosystemConventionStatus::Active,
            Some(false) => {}
            None => saw_unverifiable = true,
        }
    }
    if saw_unverifiable {
        FlutterEcosystemConventionStatus::UnverifiableVersion
    } else {
        FlutterEcosystemConventionStatus::UnsupportedVersion
    }
}

fn supported_majors(convention: FlutterEcosystemConvention) -> &'static [u64] {
    match convention {
        FlutterEcosystemConvention::GoRouter => &[14, 15, 16, 17],
        FlutterEcosystemConvention::Provider => &[6],
        FlutterEcosystemConvention::FlutterRiverpod => &[2, 3],
        FlutterEcosystemConvention::FlutterBloc => &[8, 9],
    }
}

fn convention_findings(
    project: &DartProjectAnalysis,
    convention: FlutterEcosystemConvention,
    package: &str,
) -> Vec<FlutterEcosystemFinding> {
    let mut findings = Vec::new();
    for file in &project.files {
        if !imports_package(file, package) {
            continue;
        }
        for invocation in &file.invocations {
            if let Some(pattern) = invocation_pattern(convention, &invocation.target) {
                findings.push(FlutterEcosystemFinding {
                    file_path: file.path.clone(),
                    pattern: pattern.to_string(),
                    kind: FlutterEcosystemFindingKind::Invocation,
                    confidence: Confidence::Medium,
                    span: invocation.span.clone(),
                });
            }
        }
        for declaration in &file.declarations {
            let Some(base_class) = declaration.extends.as_deref() else {
                continue;
            };
            if let Some(pattern) = base_class_pattern(convention, base_class) {
                findings.push(FlutterEcosystemFinding {
                    file_path: file.path.clone(),
                    pattern: pattern.to_string(),
                    kind: FlutterEcosystemFindingKind::BaseClass,
                    confidence: Confidence::Medium,
                    span: declaration.span.clone(),
                });
            }
        }
    }
    findings.sort_by(|left, right| {
        (
            &left.file_path,
            left.span.byte_start,
            left.span.byte_end,
            left.kind,
            &left.pattern,
        )
            .cmp(&(
                &right.file_path,
                right.span.byte_start,
                right.span.byte_end,
                right.kind,
                &right.pattern,
            ))
    });
    findings.dedup();
    findings
}

fn imports_package(file: &DartFileAnalysis, package: &str) -> bool {
    let prefix = format!("package:{package}/");
    file.imports
        .iter()
        .any(|import| import.uri.starts_with(prefix.as_str()))
}

fn invocation_pattern(
    convention: FlutterEcosystemConvention,
    target: &str,
) -> Option<&'static str> {
    let last = target.rsplit('.').next()?;
    match convention {
        FlutterEcosystemConvention::GoRouter => match last {
            "GoRouter" => Some("GoRouter"),
            "GoRoute" => Some("GoRoute"),
            "ShellRoute" => Some("ShellRoute"),
            "StatefulShellRoute" => Some("StatefulShellRoute"),
            "StatefulShellBranch" => Some("StatefulShellBranch"),
            _ => None,
        },
        FlutterEcosystemConvention::Provider => match last {
            "Provider" => Some("Provider"),
            "ChangeNotifierProvider" => Some("ChangeNotifierProvider"),
            "MultiProvider" => Some("MultiProvider"),
            "ProxyProvider" => Some("ProxyProvider"),
            "Consumer" => Some("Consumer"),
            "Selector" => Some("Selector"),
            "watch" if target.contains('.') => Some("BuildContext.watch"),
            "read" if target.contains('.') => Some("BuildContext.read"),
            "select" if target.contains('.') => Some("BuildContext.select"),
            _ => None,
        },
        FlutterEcosystemConvention::FlutterRiverpod => match last {
            "ProviderScope" => Some("ProviderScope"),
            "Consumer" => Some("Consumer"),
            _ => None,
        },
        FlutterEcosystemConvention::FlutterBloc => match last {
            "BlocProvider" => Some("BlocProvider"),
            "MultiBlocProvider" => Some("MultiBlocProvider"),
            "BlocBuilder" => Some("BlocBuilder"),
            "BlocListener" => Some("BlocListener"),
            "BlocConsumer" => Some("BlocConsumer"),
            "BlocSelector" => Some("BlocSelector"),
            "RepositoryProvider" => Some("RepositoryProvider"),
            "MultiRepositoryProvider" => Some("MultiRepositoryProvider"),
            _ => None,
        },
    }
}

fn base_class_pattern(
    convention: FlutterEcosystemConvention,
    base_class: &str,
) -> Option<&'static str> {
    if convention != FlutterEcosystemConvention::FlutterRiverpod {
        return None;
    }
    match base_class.rsplit('.').next()? {
        "ConsumerWidget" => Some("ConsumerWidget"),
        "ConsumerStatefulWidget" => Some("ConsumerStatefulWidget"),
        "ConsumerState" => Some("ConsumerState"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

fn constraint_intersects_supported_majors(
    constraint: &str,
    supported_majors: &[u64],
) -> Option<bool> {
    let constraint = constraint.trim();
    if constraint.is_empty() || constraint == "any" || constraint.contains("||") {
        return None;
    }
    if let Some(version) = constraint.strip_prefix('^').and_then(parse_version) {
        return Some(supported_majors.contains(&version.major));
    }
    if !constraint.chars().any(char::is_whitespace)
        && !matches!(constraint.as_bytes().first(), Some(b'>' | b'<' | b'='))
    {
        return parse_version(constraint).map(|version| supported_majors.contains(&version.major));
    }

    let comparators = constraint
        .replace(',', " ")
        .split_whitespace()
        .map(parse_comparator)
        .collect::<Option<Vec<_>>>()?;
    if comparators.is_empty() {
        return None;
    }
    Some(
        supported_majors
            .iter()
            .copied()
            .any(|major| comparators_intersect_major(&comparators, major)),
    )
}

fn comparators_intersect_major(comparators: &[Comparator], major: u64) -> bool {
    let mut lower = Bound {
        version: Version {
            major,
            minor: 0,
            patch: 0,
        },
        inclusive: true,
    };
    let mut upper = Bound {
        version: Version {
            major: major.saturating_add(1),
            minor: 0,
            patch: 0,
        },
        inclusive: false,
    };

    for comparator in comparators {
        match *comparator {
            Comparator::GreaterThan(version) => {
                lower = max_lower(
                    lower,
                    Bound {
                        version,
                        inclusive: false,
                    },
                );
            }
            Comparator::GreaterThanOrEqual(version) => {
                lower = max_lower(
                    lower,
                    Bound {
                        version,
                        inclusive: true,
                    },
                );
            }
            Comparator::LessThan(version) => {
                upper = min_upper(
                    upper,
                    Bound {
                        version,
                        inclusive: false,
                    },
                );
            }
            Comparator::LessThanOrEqual(version) => {
                upper = min_upper(
                    upper,
                    Bound {
                        version,
                        inclusive: true,
                    },
                );
            }
            Comparator::Equal(version) => {
                return version.major == major
                    && comparators
                        .iter()
                        .all(|comparator| comparator.matches(version));
            }
        }
    }

    lower.version < upper.version
        || (lower.version == upper.version && lower.inclusive && upper.inclusive)
}

#[derive(Debug, Clone, Copy)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn max_lower(left: Bound, right: Bound) -> Bound {
    match left.version.cmp(&right.version) {
        std::cmp::Ordering::Less => right,
        std::cmp::Ordering::Greater => left,
        std::cmp::Ordering::Equal => Bound {
            version: left.version,
            inclusive: left.inclusive && right.inclusive,
        },
    }
}

fn min_upper(left: Bound, right: Bound) -> Bound {
    match left.version.cmp(&right.version) {
        std::cmp::Ordering::Less => left,
        std::cmp::Ordering::Greater => right,
        std::cmp::Ordering::Equal => Bound {
            version: left.version,
            inclusive: left.inclusive && right.inclusive,
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum Comparator {
    GreaterThan(Version),
    GreaterThanOrEqual(Version),
    LessThan(Version),
    LessThanOrEqual(Version),
    Equal(Version),
}

impl Comparator {
    fn matches(self, version: Version) -> bool {
        match self {
            Self::GreaterThan(bound) => version > bound,
            Self::GreaterThanOrEqual(bound) => version >= bound,
            Self::LessThan(bound) => version < bound,
            Self::LessThanOrEqual(bound) => version <= bound,
            Self::Equal(bound) => version == bound,
        }
    }
}

fn parse_comparator(value: &str) -> Option<Comparator> {
    if let Some(version) = value.strip_prefix(">=").and_then(parse_version) {
        return Some(Comparator::GreaterThanOrEqual(version));
    }
    if let Some(version) = value.strip_prefix("<=").and_then(parse_version) {
        return Some(Comparator::LessThanOrEqual(version));
    }
    if let Some(version) = value.strip_prefix('>').and_then(parse_version) {
        return Some(Comparator::GreaterThan(version));
    }
    if let Some(version) = value.strip_prefix('<').and_then(parse_version) {
        return Some(Comparator::LessThan(version));
    }
    if let Some(version) = value.strip_prefix('=').and_then(parse_version) {
        return Some(Comparator::Equal(version));
    }
    parse_version(value).map(Comparator::Equal)
}

fn parse_version(value: &str) -> Option<Version> {
    let value = value.trim().trim_start_matches('v');
    let value = value.split_once('+').map_or(value, |(version, _)| version);
    let value = value.split_once('-').map_or(value, |(version, _)| version);
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_pub_constraint_shapes() {
        assert_eq!(
            constraint_intersects_supported_majors("^17.3.0", &[14, 15, 16, 17]),
            Some(true)
        );
        assert_eq!(
            constraint_intersects_supported_majors(">=14.0.0 <18.0.0", &[14, 15, 16, 17]),
            Some(true)
        );
        assert_eq!(
            constraint_intersects_supported_majors("18.0.0", &[14, 15, 16, 17]),
            Some(false)
        );
        assert_eq!(constraint_intersects_supported_majors("any", &[6]), None);
    }
}
