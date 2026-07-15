use dartscope_core::SourceSpan;

use crate::source_lines::source_lines;

#[derive(Debug, Default)]
pub(crate) struct PubspecSyntaxCheck {
    bare_wildcard_lines: Vec<usize>,
    invalid_flow_spans: Vec<SourceSpan>,
}

impl PubspecSyntaxCheck {
    pub(crate) fn is_bare_wildcard_line(&self, line: usize) -> bool {
        self.bare_wildcard_lines.contains(&line)
    }

    pub(crate) fn invalid_flow_spans(&self) -> &[SourceSpan] {
        &self.invalid_flow_spans
    }
}

pub(crate) fn check_pubspec_syntax(source: &str) -> PubspecSyntaxCheck {
    let mut check = PubspecSyntaxCheck::default();
    let mut in_dependency_section = false;
    let mut direct_indent = None;

    for source_line in source_lines(source) {
        let yaml = strip_yaml_comment(source_line.text);
        let trimmed = yaml.trim();
        if trimmed.is_empty() {
            continue;
        }

        let indent = leading_space_count(source_line.text);
        if indent == 0 {
            in_dependency_section = is_dependency_section(trimmed);
            direct_indent = None;
            continue;
        }
        if !in_dependency_section {
            continue;
        }

        let expected_indent = *direct_indent.get_or_insert(indent);
        if indent < expected_indent {
            in_dependency_section = false;
            direct_indent = None;
            continue;
        }
        if indent != expected_indent {
            continue;
        }

        let Some(colon) = find_unquoted_colon(trimmed) else {
            continue;
        };
        let value = trimmed[colon + 1..].trim();
        if value == "*" {
            check.bare_wildcard_lines.push(source_line.number);
        }
        if value.starts_with('{') && !flow_delimiters_are_balanced(value) {
            check.invalid_flow_spans.push(SourceSpan::line(
                source_line.number,
                source_line.byte_start,
                source_line.text,
            ));
        }
    }

    check
}

fn is_dependency_section(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "dependencies:" | "dev_dependencies:" | "dependency_overrides:"
    )
}

fn flow_delimiters_are_balanced(value: &str) -> bool {
    let mut delimiters = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut chars = value.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if active_quote == '\'' && ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' | '[' => delimiters.push(ch),
            '}' => {
                if delimiters.pop() != Some('{') {
                    return false;
                }
                if delimiters.is_empty() && !value[index + ch.len_utf8()..].trim().is_empty() {
                    return false;
                }
            }
            ']' => {
                if delimiters.pop() != Some('[') {
                    return false;
                }
            }
            _ => {}
        }
    }

    quote.is_none() && !escaped && delimiters.is_empty()
}

fn find_unquoted_colon(value: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                ':' => return Some(index),
                _ => {}
            }
        }
    }
    None
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;

    for (index, ch) in line.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if active_quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '#' if previous.is_none_or(char::is_whitespace) => return &line[..index],
                _ => {}
            }
        }
        previous = Some(ch);
    }

    line
}

fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_bare_wildcards_but_not_named_aliases() {
        let source = concat!(
            "dependencies:\n",
            "  wildcard: *\n",
            "  alias: *defaults\n",
        );
        let check = check_pubspec_syntax(source);

        assert!(check.is_bare_wildcard_line(2));
        assert!(!check.is_bare_wildcard_line(3));
    }

    #[test]
    fn rejects_unbalanced_flow_delimiters_and_quotes() {
        for value in [
            "{ path: ../local } }",
            "{ git: { url: https://example.com/repo.git ] }",
            "{ path: \"unterminated }",
        ] {
            assert!(!flow_delimiters_are_balanced(value), "{value}");
        }
    }

    #[test]
    fn accepts_nested_flow_mappings_with_quoted_commas() {
        assert!(flow_delimiters_are_balanced(
            "{ git: { url: \"https://example.com/repo.git?parts=one,two\", ref: stable } }"
        ));
    }
}
