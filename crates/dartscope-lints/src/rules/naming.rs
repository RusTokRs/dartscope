use dartscope_core::{DartDeclarationKind, normalize_path};

use crate::context::RuleContext;
use crate::rules::diagnostic;
use crate::{DartLintConfig, DartLintDiagnostic, DartLintRuleId};

pub(crate) fn run(
    context: &RuleContext<'_>,
    config: &DartLintConfig,
    diagnostics: &mut Vec<DartLintDiagnostic>,
) {
    let severity = config.severity(DartLintRuleId::NamingConvention);
    for file in &context.project.files {
        if ignored(&file.path, &config.naming.ignored_path_prefixes) {
            continue;
        }
        if config.naming.check_file_names && !valid_dart_file_name(&file.path) {
            diagnostics.push(diagnostic(
                DartLintRuleId::NamingConvention,
                severity,
                "Dart file names must use lower_snake_case segments",
                file.path.clone(),
                None,
                Vec::new(),
            ));
        }
        if !config.naming.check_top_level_declarations {
            continue;
        }
        for declaration in &file.declarations {
            if declaration.parent_symbol_id.is_some()
                || declaration.name.starts_with('<')
                || valid_declaration_name(declaration.kind, &declaration.name)
            {
                continue;
            }
            diagnostics.push(diagnostic(
                DartLintRuleId::NamingConvention,
                severity,
                format!(
                    "top-level {} `{}` does not follow the configured Dart naming convention",
                    kind_label(declaration.kind),
                    declaration.name
                ),
                file.path.clone(),
                Some(declaration.span.clone()),
                Vec::new(),
            ));
        }
    }
}

fn ignored(path: &str, prefixes: &[String]) -> bool {
    prefixes
        .iter()
        .any(|prefix| path.starts_with(&normalize_path(prefix.clone())))
}

fn valid_dart_file_name(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next() else {
        return true;
    };
    let Some(stem) = file_name.strip_suffix(".dart") else {
        return true;
    };
    stem.split('.').all(is_lower_snake_case)
}

fn is_lower_snake_case(value: &str) -> bool {
    if value.is_empty() || !value.is_ascii() {
        return true;
    }
    let bytes = value.as_bytes();
    bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        && !value.ends_with('_')
        && !value.contains("__")
}

fn valid_declaration_name(kind: DartDeclarationKind, name: &str) -> bool {
    match kind {
        DartDeclarationKind::Class
        | DartDeclarationKind::Mixin
        | DartDeclarationKind::Enum
        | DartDeclarationKind::Extension
        | DartDeclarationKind::ExtensionType
        | DartDeclarationKind::Typedef => is_upper_camel_case(name),
        DartDeclarationKind::Function | DartDeclarationKind::Variable => is_lower_camel_case(name),
        _ => true,
    }
}

fn is_upper_camel_case(value: &str) -> bool {
    let value = value.trim_start_matches('_');
    if value.is_empty() || !value.is_ascii() || value.contains('_') {
        return !value.is_ascii();
    }
    value.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric())
}

fn is_lower_camel_case(value: &str) -> bool {
    let value = value.trim_start_matches('_');
    if value.is_empty() || !value.is_ascii() || value.contains('_') {
        return !value.is_ascii();
    }
    value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric())
}

fn kind_label(kind: DartDeclarationKind) -> &'static str {
    match kind {
        DartDeclarationKind::Class => "class",
        DartDeclarationKind::Mixin => "mixin",
        DartDeclarationKind::Enum => "enum",
        DartDeclarationKind::Extension => "extension",
        DartDeclarationKind::ExtensionType => "extension type",
        DartDeclarationKind::Typedef => "typedef",
        DartDeclarationKind::Function => "function",
        DartDeclarationKind::Variable => "variable",
        _ => "declaration",
    }
}
