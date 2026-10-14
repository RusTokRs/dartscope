use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLintConfig, DartLintDiagnostic, DartLintRuleId};

/// `fromEnvironment` receivers recognized by the Dart language.
const SOURCE_TYPES: [&str; 4] = ["bool", "int", "double", "String"];

/// Reports `X.fromEnvironment("name")` reads whose entry is neither provided
/// in the configured environment nor protected by a `defaultValue`.
///
/// The rule is inert while the configuration carries no environment entries:
/// without a provided define set, every read is Dart's own responsibility and
/// flagging it would be fabricated. Only literal, positional first arguments
/// are inspected; dynamic names are never guessed.
pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    if config.environment.is_empty() {
        return;
    }
    let severity = config.severity(DartLintRuleId::UndefinedDefine);
    for file in &context.project.files {
        if !context.includes_path(&file.path) {
            continue;
        }
        for invocation in &file.invocations {
            let Some(source_type) = SOURCE_TYPES
                .iter()
                .find(|ty| invocation.target == format!("{ty}.fromEnvironment"))
            else {
                continue;
            };
            let Some(name_argument) = invocation.arguments.first() else {
                continue;
            };
            if name_argument.name.is_some() {
                continue;
            }
            let Some(define_name) = name_argument.string_value.as_deref() else {
                continue;
            };
            let has_default = invocation
                .arguments
                .iter()
                .any(|argument| argument.name.as_deref() == Some("defaultValue"));
            if has_default || config.environment.contains_key(define_name) {
                continue;
            }
            diagnostics.push(diagnostic(
                DartLintRuleId::UndefinedDefine,
                severity,
                format!(
                    "environment define {define_name:?} is read via {source_type}.fromEnvironment without a default and was not provided"
                ),
                file.path.clone(),
                Some(invocation.span.clone()),
                Vec::new(),
            ));
        }
    }
}
