use dartscope_core::normalize_path;

use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartImportPatternKind, DartLintConfig, DartLintDiagnostic, DartLintRuleId};

pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    let mut patterns = config.forbidden_imports.clone();
    patterns.sort_by(|left, right| {
        (left.source_prefix.as_deref(), &left.uri, left.match_kind).cmp(&(
            right.source_prefix.as_deref(),
            &right.uri,
            right.match_kind,
        ))
    });
    let severity = config.severity(DartLintRuleId::ForbiddenImport);

    for file in &context.project.files {
        if !context.includes_path(&file.path) {
            continue;
        }
        for import in &file.imports {
            for pattern in &patterns {
                if !source_matches(&file.path, pattern.source_prefix.as_deref())
                    || !uri_matches(&import.uri, &pattern.uri, pattern.match_kind)
                {
                    continue;
                }
                diagnostics.push(diagnostic(
                    DartLintRuleId::ForbiddenImport,
                    severity,
                    format!(
                        "import `{}` is forbidden by pattern `{}`",
                        import.uri, pattern.uri
                    ),
                    file.path.clone(),
                    Some(import.span.clone()),
                    Vec::new(),
                ));
            }
        }
    }
}

fn source_matches(path: &str, source_prefix: Option<&str>) -> bool {
    source_prefix
        .map(|prefix| path.starts_with(&normalize_path(prefix.to_string())))
        .unwrap_or(true)
}

fn uri_matches(uri: &str, pattern: &str, kind: DartImportPatternKind) -> bool {
    match kind {
        DartImportPatternKind::Exact => uri == pattern,
        DartImportPatternKind::Prefix => uri.starts_with(pattern),
    }
}
