use dartscope_core::{
    DartDiagnostic, DartExport, DartImport, DartNamespaceCombinator, DartNamespaceCombinatorKind,
    DartUriConfiguration, SourceSpan,
};

use crate::declarations::{is_identifier, quoted_value};
use crate::source_lines::{source_lines, SourceLine};

struct PendingNamespaceDirective {
    keyword: &'static str,
    text: String,
    start_line: usize,
    start_byte: usize,
}

#[derive(Default)]
struct NamespaceDirectiveOutput {
    imports: Vec<DartImport>,
    exports: Vec<DartExport>,
    diagnostics: Vec<DartDiagnostic>,
}

pub(crate) fn extract_namespace_directives(
    source: &str,
    masked_source: &str,
) -> (Vec<DartImport>, Vec<DartExport>, Vec<DartDiagnostic>) {
    let mut output = NamespaceDirectiveOutput::default();
    let mut pending: Option<PendingNamespaceDirective> = None;
    let mut last_source_line: Option<SourceLine<'_>> = None;

    for (source_line, masked_line) in source_lines(source)
        .into_iter()
        .zip(source_lines(masked_source))
    {
        let line = source_line.text;
        let trimmed = line.trim();
        let masked_trimmed = masked_line.text.trim();
        last_source_line = Some(source_line);

        if let Some(directive) = pending.as_mut() {
            directive.text.push(' ');
            directive.text.push_str(trimmed);
        } else if starts_directive(masked_trimmed, "import") {
            pending = Some(PendingNamespaceDirective {
                keyword: "import",
                text: trimmed.to_string(),
                start_line: source_line.number,
                start_byte: source_line.byte_start,
            });
        } else if starts_directive(masked_trimmed, "export") {
            pending = Some(PendingNamespaceDirective {
                keyword: "export",
                text: trimmed.to_string(),
                start_line: source_line.number,
                start_byte: source_line.byte_start,
            });
        }

        if masked_trimmed.contains(';') {
            if let Some(directive) = pending.take() {
                finish_namespace_directive(
                    directive,
                    source_line.number,
                    source_line.byte_end(),
                    line,
                    true,
                    &mut output,
                );
            }
        }
    }

    if let Some(directive) = pending {
        let last_source_line = last_source_line.unwrap_or(SourceLine {
            number: directive.start_line,
            text: "",
            byte_start: directive.start_byte,
        });
        finish_namespace_directive(
            directive,
            last_source_line.number,
            last_source_line.byte_end(),
            last_source_line.text,
            false,
            &mut output,
        );
    }

    (output.imports, output.exports, output.diagnostics)
}

fn finish_namespace_directive(
    pending: PendingNamespaceDirective,
    end_line: usize,
    end_byte: usize,
    end_text: &str,
    terminated: bool,
    output: &mut NamespaceDirectiveOutput,
) {
    let span = SourceSpan {
        byte_start: pending.start_byte,
        byte_end: end_byte,
        start_line: pending.start_line,
        start_column: 1,
        end_line,
        end_column: end_text.chars().count() + 1,
    };
    if let Some(directive) = namespace_directive(&pending.text, pending.keyword) {
        if pending.keyword == "import" {
            output.imports.push(DartImport {
                uri: directive.uri,
                configurations: directive.configurations,
                is_deferred: directive.is_deferred,
                prefix: directive.prefix,
                combinators: directive.combinators,
                span: span.clone(),
            });
        } else {
            output.exports.push(DartExport {
                uri: directive.uri,
                configurations: directive.configurations,
                combinators: directive.combinators,
                span: span.clone(),
            });
        }
    }
    if !terminated {
        output.diagnostics.push(DartDiagnostic::warning(
            "directive_missing_semicolon",
            "Dart import/export directive appears to be missing a semicolon",
            Some(span),
        ));
    }
}

fn starts_directive(line: &str, keyword: &str) -> bool {
    line == keyword
        || line
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

pub(crate) fn directive_uri(trimmed: &str, keyword: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    quoted_value(rest)
}

#[derive(Debug, Default)]
struct ParsedNamespaceDirective {
    uri: String,
    configurations: Vec<DartUriConfiguration>,
    is_deferred: bool,
    prefix: Option<String>,
    combinators: Vec<DartNamespaceCombinator>,
}

fn namespace_directive(trimmed: &str, keyword: &str) -> Option<ParsedNamespaceDirective> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let (uri, suffix) = quoted_value_with_suffix(rest)?;
    let (configurations, suffix) = uri_configurations(suffix);
    let tokens = directive_suffix_tokens(suffix);
    let mut directive = ParsedNamespaceDirective {
        uri,
        configurations,
        ..ParsedNamespaceDirective::default()
    };
    let mut index = 0;

    while index < tokens.len() {
        match tokens[index] {
            "deferred" if keyword == "import" => {
                directive.is_deferred = true;
                index += 1;
            }
            "as" if keyword == "import" => {
                directive.prefix = tokens
                    .get(index + 1)
                    .filter(|name| is_identifier(name))
                    .map(|name| (*name).to_string());
                index += 2;
            }
            "show" | "hide" => {
                let kind = if tokens[index] == "show" {
                    DartNamespaceCombinatorKind::Show
                } else {
                    DartNamespaceCombinatorKind::Hide
                };
                index += 1;
                let start = index;
                while index < tokens.len() && !matches!(tokens[index], "show" | "hide") {
                    index += 1;
                }
                let names = tokens[start..index]
                    .iter()
                    .filter(|name| is_identifier(name))
                    .map(|name| (*name).to_string())
                    .collect();
                directive
                    .combinators
                    .push(DartNamespaceCombinator { kind, names });
            }
            _ => index += 1,
        }
    }

    Some(directive)
}

fn uri_configurations(mut suffix: &str) -> (Vec<DartUriConfiguration>, &str) {
    let mut configurations = Vec::new();
    loop {
        suffix = suffix.trim_start();
        let Some(after_if) = suffix.strip_prefix("if") else {
            break;
        };
        if !after_if.starts_with(char::is_whitespace) && !after_if.starts_with('(') {
            break;
        }
        let after_if = after_if.trim_start();
        let Some(condition_start) = after_if.strip_prefix('(') else {
            break;
        };
        let Some(condition_end) = condition_start.find(')') else {
            break;
        };
        let condition = condition_start[..condition_end].trim();
        if condition.is_empty() {
            break;
        }
        let after_condition = &condition_start[condition_end + 1..];
        let Some((uri, remaining)) = quoted_value_with_suffix(after_condition) else {
            break;
        };
        configurations.push(DartUriConfiguration {
            condition: condition.to_string(),
            uri,
        });
        suffix = remaining;
    }
    (configurations, suffix)
}

fn quoted_value_with_suffix(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let quote_index = usize::from(input.starts_with('r'));
    let quote = *input.as_bytes().get(quote_index)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let rest = &input[quote_index + 1..];
    let end = rest.find(quote as char)?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

fn directive_suffix_tokens(suffix: &str) -> Vec<&str> {
    suffix
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';'))
        .filter(|token| !token.is_empty())
        .collect()
}
