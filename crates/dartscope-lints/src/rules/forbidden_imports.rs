use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLintConfig, DartLintDiagnostic, DartLintRuleId};

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
        // An `export` re-publishes a forbidden library as surely as an `import` uses it, and a
        // conditional alternative (`if (dart.library.io) 'package:forbidden/io.dart'`) is a
        // dependency in every build where its condition holds.
        let directives = file
            .imports
            .iter()
            .map(|import| ("import", &import.uri, &import.configurations, &import.span))
            .chain(
                file.exports
                    .iter()
                    .map(|export| ("export", &export.uri, &export.configurations, &export.span)),
            );
        for (keyword, uri, configurations, span) in directives {
            let uris = std::iter::once(uri).chain(
                configurations
                    .iter()
                    .map(|configuration| &configuration.uri),
            );
            for uri in uris {
                for pattern in &patterns {
                    if !source_matches(config, &file.path, pattern.source_prefix.as_deref())
                        || !pattern.match_kind.matches(uri, &pattern.uri)
                    {
                        continue;
                    }
                    diagnostics.push(diagnostic(
                        DartLintRuleId::ForbiddenImport,
                        severity,
                        format!(
                            "{keyword} `{uri}` is forbidden by pattern `{}`",
                            pattern.uri
                        ),
                        file.path.clone(),
                        Some(span.clone()),
                        Vec::new(),
                    ));
                }
            }
        }
    }
}

fn source_matches(config: &DartLintConfig, path: &str, source_prefix: Option<&str>) -> bool {
    source_prefix
        .map(|prefix| config.path_has_prefix(path, prefix))
        .unwrap_or(true)
}
