use dartscope_core::{
    DartEnclosingSymbol, DartEnclosingSymbolKind, DartGraphqlClientCall, DartGraphqlOperation,
    DartGraphqlOperationType, DartGraphqlOperationUse, SourceSpan,
};

use crate::declarations::{is_identifier, next_identifier, variable_name_after_keyword};
use crate::source_lines::{source_lines, SourceLine};

pub(crate) fn extract_graphql_operations(
    source: &str,
    masked_source: &str,
) -> Vec<DartGraphqlOperation> {
    let mut operations = Vec::new();
    let lines = source_lines(source);
    let masked_lines = source_lines(masked_source);
    let mut index = 0usize;

    while let Some(source_line) = lines.get(index).copied() {
        let line = source_line.text;
        let trimmed = line.trim();
        let masked_trimmed = masked_lines[index].text.trim();
        let Some((constant_name, delimiter)) = graphql_document_start(masked_trimmed, trimmed)
        else {
            index += 1;
            continue;
        };

        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);
        let mut document = String::new();
        let mut ended_on_start_line = false;
        if let Some(after_start) = trimmed.split_once(delimiter).map(|(_, right)| right) {
            if let Some((before_end, _)) = after_start.split_once(delimiter) {
                document.push_str(before_end);
                ended_on_start_line = true;
            } else {
                document.push_str(after_start);
                document.push('\n');
            }
        }

        index += 1;
        if !ended_on_start_line {
            while let Some(document_line) = lines.get(index).copied() {
                if let Some((before_end, _)) = document_line.text.split_once(delimiter) {
                    document.push_str(before_end);
                    index += 1;
                    break;
                }
                document.push_str(document_line.text);
                document.push('\n');
                index += 1;
            }
        }

        if let Some(operation) = graphql_operation_from_document(constant_name, &document, span) {
            operations.push(operation);
        }
    }

    operations
}

fn graphql_document_start(
    masked_trimmed: &str,
    source_trimmed: &str,
) -> Option<(String, &'static str)> {
    if !masked_trimmed.starts_with("const ") && !masked_trimmed.starts_with("final ") {
        return None;
    }
    let (left, _) = masked_trimmed.split_once('=')?;
    let (_, right) = source_trimmed.split_once('=')?;
    let constant_name = variable_name_from_declaration_left(left)?;
    let right = right.trim_start();
    let delimiter = if right.starts_with("r'''") || right.starts_with("'''") {
        "'''"
    } else if right.starts_with("r\"\"\"") || right.starts_with("\"\"\"") {
        "\"\"\""
    } else {
        return None;
    };
    Some((constant_name, delimiter))
}

fn variable_name_from_declaration_left(left: &str) -> Option<String> {
    ["const", "final"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(left.trim(), keyword))
}

fn graphql_operation_from_document(
    constant_name: String,
    document: &str,
    span: SourceSpan,
) -> Option<DartGraphqlOperation> {
    let document = document.trim();
    let (operation_type, rest) = if let Some(rest) = document.strip_prefix("query") {
        (DartGraphqlOperationType::Query, rest)
    } else if let Some(rest) = document.strip_prefix("mutation") {
        (DartGraphqlOperationType::Mutation, rest)
    } else if let Some(rest) = document.strip_prefix("subscription") {
        (DartGraphqlOperationType::Subscription, rest)
    } else {
        return None;
    };

    let operation_name = graphql_operation_name(rest);
    let variable_names = graphql_operation_variable_names(rest);
    let root_fields = graphql_root_fields(document);

    Some(DartGraphqlOperation {
        constant_name,
        operation_type,
        operation_name,
        variable_names,
        root_fields,
        span,
    })
}

fn graphql_operation_name(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    if rest.starts_with('{') || rest.is_empty() {
        return None;
    }
    next_identifier(rest)
}

fn graphql_operation_variable_names(rest: &str) -> Vec<String> {
    let Some(start) = rest.find('(') else {
        return Vec::new();
    };
    let before_selection = rest.find('{').unwrap_or(rest.len());
    if start > before_selection {
        return Vec::new();
    }
    let Some(end) = rest[start + 1..].find(')') else {
        return Vec::new();
    };
    let variables_block = &rest[start + 1..start + 1 + end];
    let mut variables = Vec::new();
    let mut chars = variables_block.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            continue;
        }
        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if !name.is_empty() {
            variables.push(name);
        }
    }

    variables.sort();
    variables.dedup();
    variables
}

fn graphql_root_fields(document: &str) -> Vec<String> {
    let Some(start) = document.find('{') else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    let mut depth = 0usize;
    let mut paren_depth = 0usize;
    let mut token = String::new();
    let mut chars = document[start..].chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                depth += 1;
                token.clear();
            }
            '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                token.clear();
            }
            '#' => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            ch if depth == 1 && (ch.is_ascii_alphabetic() || ch == '_') => {
                if paren_depth > 0 {
                    continue;
                }
                token.push(ch);
                while let Some(next) = chars.peek().copied() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        token.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !matches!(token.as_str(), "fragment" | "on") {
                    fields.push(token.clone());
                }
                token.clear();
            }
            '(' if depth == 1 => {
                paren_depth += 1;
                token.clear();
            }
            ')' if depth == 1 => {
                paren_depth = paren_depth.saturating_sub(1);
                token.clear();
            }
            _ => {
                token.clear();
            }
        }
    }

    fields.sort();
    fields.dedup();
    fields
}

pub(crate) fn extract_graphql_operation_uses(
    source: &str,
    masked_source: &str,
) -> Vec<DartGraphqlOperationUse> {
    let mut uses = Vec::new();
    let mut brace_depth = 0usize;
    let mut symbol_stack: Vec<(usize, DartEnclosingSymbol)> = Vec::new();
    let mut pending_symbol: Option<DartEnclosingSymbol> = None;
    let mut pending_client_call: Option<DartGraphqlClientCall> = None;
    let lines = source_lines(source);
    let masked_lines = source_lines(masked_source);

    for (index, source_line) in lines.iter().copied().enumerate() {
        let line = source_line.text;
        let span = SourceSpan::line(source_line.number, source_line.byte_start, line);
        let trimmed = masked_lines[index].text.trim();

        while symbol_stack
            .last()
            .is_some_and(|(parent_depth, _)| brace_depth <= *parent_depth)
        {
            symbol_stack.pop();
        }

        if let Some(callable) = callable_signature_name_from_line(trimmed) {
            pending_symbol = Some(DartEnclosingSymbol {
                name: callable,
                kind: DartEnclosingSymbolKind::Callable,
            });
        } else if brace_depth == 0 {
            if let Some(variable) = top_level_variable_owner_from_line(trimmed) {
                pending_symbol = Some(DartEnclosingSymbol {
                    name: variable,
                    kind: DartEnclosingSymbolKind::Variable,
                });
            }
        }
        if trimmed.ends_with('{') {
            let line_symbol = callable_name_from_line(trimmed).map(|name| DartEnclosingSymbol {
                name,
                kind: DartEnclosingSymbolKind::Callable,
            });
            if let Some(symbol) = line_symbol.or_else(|| pending_symbol.take()) {
                symbol_stack.push((brace_depth, symbol));
            }
        }

        if let Some(client_call) = graphql_client_call_from_line(trimmed) {
            pending_client_call = Some(client_call);
        }

        if let Some(constant_name) = gql_constant_from_line(trimmed) {
            let enclosing_symbol = symbol_stack.last().map(|(_, symbol)| symbol.clone());
            uses.push(DartGraphqlOperationUse {
                constant_name,
                client_call: pending_client_call.unwrap_or(DartGraphqlClientCall::Unknown),
                variable_names: graphql_variable_names_from_lines(&lines, &masked_lines, index),
                enclosing_callable: enclosing_symbol
                    .as_ref()
                    .filter(|symbol| symbol.kind == DartEnclosingSymbolKind::Callable)
                    .map(|symbol| symbol.name.clone()),
                enclosing_symbol,
                span,
            });
            pending_client_call = None;
        }

        let opens = line.chars().filter(|c| *c == '{').count();
        let closes = line.chars().filter(|c| *c == '}').count();
        brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
    }

    uses
}

fn graphql_variable_names_from_lines(
    lines: &[SourceLine<'_>],
    masked_lines: &[SourceLine<'_>],
    start_index: usize,
) -> Vec<String> {
    let mut variables = Vec::new();
    let mut in_variables = false;
    let mut map_depth = 0usize;

    for (source_line, masked_line) in lines
        .iter()
        .skip(start_index)
        .zip(masked_lines.iter().skip(start_index))
        .take(80)
    {
        let trimmed = source_line.text.trim();
        let masked_trimmed = masked_line.text.trim();
        let mut scan = trimmed;
        let mut masked_scan = masked_trimmed;

        if !in_variables {
            let Some((_, masked_after_marker)) = masked_trimmed.split_once("variables:") else {
                if masked_trimmed == ")," || masked_trimmed == ")" {
                    break;
                }
                continue;
            };
            let Some(open_index) = masked_after_marker.find('{') else {
                continue;
            };
            let Some((_, source_after_marker)) = trimmed.split_once("variables:") else {
                continue;
            };
            scan = &source_after_marker[open_index + 1..];
            masked_scan = &masked_after_marker[open_index + 1..];
            in_variables = true;
            map_depth = 1;
        }

        collect_top_level_map_keys(scan, masked_scan, map_depth, &mut variables);
        for ch in masked_scan.chars() {
            match ch {
                '{' => map_depth += 1,
                '}' => {
                    map_depth = map_depth.saturating_sub(1);
                    if map_depth == 0 {
                        variables.sort();
                        variables.dedup();
                        return variables;
                    }
                }
                _ => {}
            }
        }
    }

    variables.sort();
    variables.dedup();
    variables
}

fn collect_top_level_map_keys(
    line: &str,
    masked_line: &str,
    initial_depth: usize,
    variables: &mut Vec<String>,
) {
    let mut depth = initial_depth;
    let chars: Vec<_> = masked_line.char_indices().collect();
    let mut index = 0usize;

    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        match ch {
            _ if depth == 1 && matches!(line.as_bytes().get(byte_index), Some(b'\'' | b'"')) => {
                let quote = line.as_bytes()[byte_index] as char;
                if let Some((key, next_index)) = quoted_map_key_at(line, byte_index, quote) {
                    variables.push(key);
                    index = chars
                        .iter()
                        .position(|(candidate, _)| *candidate >= next_index)
                        .unwrap_or(chars.len());
                    continue;
                }
            }
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
}

fn quoted_map_key_at(line: &str, start: usize, quote: char) -> Option<(String, usize)> {
    let rest = &line[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    let key = &rest[..end];
    let after_key_index = start + quote.len_utf8() + end + quote.len_utf8();
    let after_key = line[after_key_index..].trim_start();
    after_key
        .starts_with(':')
        .then(|| (key.to_string(), after_key_index))
}

fn graphql_client_call_from_line(trimmed: &str) -> Option<DartGraphqlClientCall> {
    if trimmed.contains(".query(") {
        Some(DartGraphqlClientCall::Query)
    } else if trimmed.contains(".mutate(") {
        Some(DartGraphqlClientCall::Mutation)
    } else if trimmed.contains(".subscribe(") {
        Some(DartGraphqlClientCall::Subscription)
    } else {
        None
    }
}

fn gql_constant_from_line(trimmed: &str) -> Option<String> {
    let marker = "gql(";
    let start = trimmed.find(marker)? + marker.len();
    let rest = trimmed[start..].trim_start();
    if rest.starts_with('\'')
        || rest.starts_with('"')
        || rest.starts_with("r'")
        || rest.starts_with("r\"")
    {
        return None;
    }
    next_identifier(rest)
}

fn callable_name_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.ends_with('{') || !trimmed.contains('(') {
        return None;
    }
    callable_signature_name_from_line(trimmed)
}

fn callable_signature_name_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.contains('(') || trimmed.ends_with(';') || trimmed.contains("=>") {
        return None;
    }
    if trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("switch ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("builder:")
    {
        return None;
    }

    let before_paren = trimmed.split_once('(')?.0.trim();
    let name = before_paren.split_whitespace().last()?;
    is_identifier(name).then_some(name.to_string())
}

fn top_level_variable_owner_from_line(trimmed: &str) -> Option<String> {
    if !trimmed.contains('=') {
        return None;
    }
    ["const", "final", "var"]
        .iter()
        .find_map(|keyword| variable_name_after_keyword(trimmed, keyword))
        .filter(|name| is_identifier(name))
}
