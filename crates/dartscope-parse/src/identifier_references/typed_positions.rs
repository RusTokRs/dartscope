use std::collections::HashSet;

use dartscope_core::{
    Confidence, DartDeclaration, DartDeclarationKind, DartFileAnalysis, DartIdentifierReference,
    DartIdentifierReferenceKind, SourceSpan,
};

use crate::source_lines::span_for_byte_range;

#[derive(Debug, Clone, Copy)]
struct IdentifierToken<'source> {
    text: &'source str,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct TypeRoot<'source> {
    token: IdentifierToken<'source>,
    prefix: Option<String>,
    confidence: Confidence,
}

pub(super) fn collect_declaration_type_references(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
) -> Vec<DartIdentifierReference> {
    let import_prefixes: HashSet<String> = analysis
        .imports
        .iter()
        .filter_map(|import| import.prefix.clone())
        .collect();
    let mut references = Vec::new();

    for declaration in &analysis.declarations {
        let type_parameters = visible_type_parameter_names(masked_source, analysis, declaration);
        if supports_return_type(declaration.kind) {
            collect_return_type(
                source,
                masked_source,
                analysis,
                declaration,
                &import_prefixes,
                &type_parameters,
                &mut references,
            );
        }
        if supports_parameters(declaration.kind) {
            collect_parameter_types(
                source,
                masked_source,
                analysis,
                declaration,
                &import_prefixes,
                &type_parameters,
                &mut references,
            );
        }
        if supports_variable_type(declaration.kind) {
            collect_variable_type(
                source,
                masked_source,
                analysis,
                declaration,
                &import_prefixes,
                &type_parameters,
                &mut references,
            );
        }
    }

    references
}

#[allow(clippy::too_many_arguments)]
fn collect_return_type(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    let header_end = declaration_header_end(masked_source, span);
    let Some(type_end) = return_type_end(masked_source, span.byte_start, header_end, declaration)
    else {
        return;
    };
    let Some(root) = type_root(
        masked_source,
        span.byte_start,
        type_end,
        import_prefixes,
        type_parameters,
    ) else {
        return;
    };
    references.push(reference_from_root(
        source,
        analysis,
        declaration.symbol_id.clone(),
        DartIdentifierReferenceKind::ReturnType,
        root,
    ));
}

#[allow(clippy::too_many_arguments)]
fn collect_parameter_types(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    let header_end = declaration_header_end(masked_source, span);
    let Some((start, end)) =
        callable_parameter_range(masked_source, span.byte_start, header_end, declaration)
    else {
        return;
    };
    collect_parameter_range(
        source,
        masked_source,
        analysis,
        declaration,
        start,
        end,
        import_prefixes,
        type_parameters,
        references,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_parameter_range(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    start: usize,
    end: usize,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    for (segment_start, segment_end) in top_level_segments(masked_source, start, end) {
        let Some((trimmed_start, trimmed_end)) =
            trim_range(masked_source, segment_start, segment_end)
        else {
            continue;
        };
        let bytes = masked_source.as_bytes();
        if matches!(
            (
                bytes.get(trimmed_start),
                bytes.get(trimmed_end.saturating_sub(1))
            ),
            (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']'))
        ) {
            collect_parameter_range(
                source,
                masked_source,
                analysis,
                declaration,
                trimmed_start + 1,
                trimmed_end - 1,
                import_prefixes,
                type_parameters,
                references,
            );
            continue;
        }

        let declaration_end =
            top_level_assignment(masked_source, trimmed_start, trimmed_end).unwrap_or(trimmed_end);
        if contains_receiver_formal(masked_source, trimmed_start, declaration_end) {
            continue;
        }
        let Some(name) = last_top_level_identifier(masked_source, trimmed_start, declaration_end)
        else {
            continue;
        };
        let Some(root) = type_root(
            masked_source,
            trimmed_start,
            declaration_end,
            import_prefixes,
            type_parameters,
        ) else {
            continue;
        };
        if root.token.end >= name.start {
            continue;
        }
        references.push(reference_from_root(
            source,
            analysis,
            declaration.symbol_id.clone(),
            DartIdentifierReferenceKind::ParameterType,
            root,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_variable_type(
    source: &str,
    masked_source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
    references: &mut Vec<DartIdentifierReference>,
) {
    let Some(span) = declaration.declaration_span.as_ref() else {
        return;
    };
    let header_end = declaration_header_end(masked_source, span);
    let declaration_end =
        top_level_assignment(masked_source, span.byte_start, header_end).unwrap_or(header_end);
    let Some(name) = last_top_level_identifier(masked_source, span.byte_start, declaration_end)
    else {
        return;
    };
    let Some(root) = type_root(
        masked_source,
        span.byte_start,
        declaration_end,
        import_prefixes,
        type_parameters,
    ) else {
        return;
    };
    if root.token.end >= name.start {
        return;
    }
    references.push(reference_from_root(
        source,
        analysis,
        declaration.parent_symbol_id.clone(),
        DartIdentifierReferenceKind::VariableType,
        root,
    ));
}

fn reference_from_root(
    source: &str,
    analysis: &DartFileAnalysis,
    enclosing_symbol_id: Option<String>,
    kind: DartIdentifierReferenceKind,
    root: TypeRoot<'_>,
) -> DartIdentifierReference {
    DartIdentifierReference {
        source_path: analysis.path.clone(),
        name: root.token.text.to_string(),
        prefix: root.prefix,
        kind,
        confidence: root.confidence,
        enclosing_symbol_id,
        span: span_for_byte_range(source, root.token.start, root.token.end),
    }
}

fn type_root<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    import_prefixes: &HashSet<String>,
    type_parameters: &HashSet<String>,
) -> Option<TypeRoot<'source>> {
    let bytes = source.as_bytes();
    let mut at = skip_whitespace(bytes, start);
    while at < end {
        let modifier = identifier_at(source, at)?;
        if !is_declaration_modifier(modifier.text) {
            break;
        }
        at = skip_whitespace(bytes, modifier.end);
    }

    let first = identifier_at(source, at)?;
    if is_non_nominal_root(first.text) {
        return None;
    }
    let dot = skip_whitespace(bytes, first.end);
    if dot < end && bytes.get(dot) == Some(&b'.') {
        let second_start = skip_whitespace(bytes, dot + 1);
        let second = identifier_at(source, second_start)?;
        if second.end > end || !import_prefixes.contains(first.text) {
            return None;
        }
        return Some(TypeRoot {
            token: second,
            prefix: Some(first.text.to_string()),
            confidence: Confidence::High,
        });
    }

    if first.end > end || type_parameters.contains(first.text) || is_sdk_root(first.text) {
        return None;
    }
    Some(TypeRoot {
        token: first,
        prefix: None,
        confidence: Confidence::Medium,
    })
}

fn visible_type_parameter_names(
    source: &str,
    analysis: &DartFileAnalysis,
    declaration: &DartDeclaration,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut current = Some(declaration);
    let mut visited = HashSet::new();
    while let Some(item) = current {
        if let Some(symbol_id) = item.symbol_id.as_deref()
            && !visited.insert(symbol_id.to_string())
        {
            break;
        }
        names.extend(declaration_type_parameter_names(source, item));
        current = item.parent_symbol_id.as_deref().and_then(|parent| {
            analysis
                .declarations
                .iter()
                .find(|candidate| candidate.symbol_id.as_deref() == Some(parent))
        });
    }
    names
}

fn declaration_type_parameter_names(
    source: &str,
    declaration: &DartDeclaration,
) -> HashSet<String> {
    let Some(span) = declaration.declaration_span.as_ref() else {
        return HashSet::new();
    };
    let end = declaration_header_end(source, span);
    let name_end = if matches!(
        declaration.kind,
        DartDeclarationKind::Function | DartDeclarationKind::Method
    ) {
        callable_name_token(source, span.byte_start, end, declaration).map(|token| token.end)
    } else if is_type_declaration(declaration.kind) {
        find_identifier_named(source, span.byte_start, end, &declaration.name)
            .map(|token| token.end)
    } else {
        None
    };
    let Some(name_end) = name_end else {
        return HashSet::new();
    };
    let open = skip_whitespace(source.as_bytes(), name_end);
    if source.as_bytes().get(open) != Some(&b'<') {
        return HashSet::new();
    }
    parse_type_parameter_block(source, open, end)
}

fn parse_type_parameter_block(source: &str, open: usize, end: usize) -> HashSet<String> {
    let bytes = source.as_bytes();
    let mut names = HashSet::new();
    let mut depth = 1usize;
    let mut expect_name = true;
    let mut at = open + 1;
    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if depth == 1 && expect_name {
                names.insert(token.text.to_string());
                expect_name = false;
            }
            at = token.end;
            continue;
        }
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => expect_name = true,
            _ => {}
        }
        at += 1;
    }
    names
}

fn return_type_end(
    source: &str,
    start: usize,
    end: usize,
    declaration: &DartDeclaration,
) -> Option<usize> {
    match declaration.kind {
        DartDeclarationKind::Function | DartDeclarationKind::Method => {
            callable_name_token(source, start, end, declaration).map(|token| token.start)
        }
        DartDeclarationKind::Getter => {
            find_identifier_named(source, start, end, "get").map(|token| token.start)
        }
        DartDeclarationKind::Operator => {
            find_identifier_named(source, start, end, "operator").map(|token| token.start)
        }
        _ => None,
    }
}

fn callable_parameter_range(
    source: &str,
    start: usize,
    end: usize,
    declaration: &DartDeclaration,
) -> Option<(usize, usize)> {
    let name_end = match declaration.kind {
        DartDeclarationKind::Operator => {
            let operator = find_identifier_named(source, start, end, "operator")?;
            find_next_byte(source, operator.end, end, b'(')?
        }
        DartDeclarationKind::Constructor => {
            let name_end = find_qualified_name_end(source, start, end, &declaration.name)?;
            let open = skip_whitespace(source.as_bytes(), name_end);
            (source.as_bytes().get(open) == Some(&b'(')).then_some(open)?
        }
        _ => {
            let token = callable_name_token(source, start, end, declaration)?;
            let mut open = skip_whitespace(source.as_bytes(), token.end);
            if source.as_bytes().get(open) == Some(&b'<') {
                open = matching_delimiter(source, open, end, b'<', b'>')? + 1;
                open = skip_whitespace(source.as_bytes(), open);
            }
            (source.as_bytes().get(open) == Some(&b'(')).then_some(open)?
        }
    };
    let close = matching_delimiter(source, name_end, end, b'(', b')')?;
    Some((name_end + 1, close))
}

fn callable_name_token<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    declaration: &DartDeclaration,
) -> Option<IdentifierToken<'source>> {
    if declaration.kind == DartDeclarationKind::Getter {
        let get = find_identifier_named(source, start, end, "get")?;
        return next_identifier(source, get.end, end)
            .filter(|token| token.text == declaration.name);
    }
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end.min(bytes.len()) {
        let Some(token) = next_identifier(source, at, end) else {
            break;
        };
        at = token.end;
        if token.text != declaration.name {
            continue;
        }
        let mut next = skip_whitespace(bytes, token.end);
        if bytes.get(next) == Some(&b'<') {
            next = matching_delimiter(source, next, end, b'<', b'>')? + 1;
            next = skip_whitespace(bytes, next);
        }
        if bytes.get(next) == Some(&b'(') {
            return Some(token);
        }
    }
    None
}

fn find_qualified_name_end(source: &str, start: usize, end: usize, name: &str) -> Option<usize> {
    let haystack = source.get(start..end)?;
    for (offset, _) in haystack.match_indices(name) {
        let absolute = start + offset;
        let before_ok = absolute == start
            || source
                .as_bytes()
                .get(absolute.saturating_sub(1))
                .is_none_or(|byte| !is_identifier_continue(*byte));
        let after = absolute + name.len();
        let after_ok = source
            .as_bytes()
            .get(after)
            .is_none_or(|byte| !is_identifier_continue(*byte));
        if before_ok && after_ok {
            let open = skip_whitespace(source.as_bytes(), after);
            if source.as_bytes().get(open) == Some(&b'(') {
                return Some(after);
            }
        }
    }
    None
}

fn declaration_header_end(source: &str, span: &SourceSpan) -> usize {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut angles = 0usize;
    let mut at = span.byte_start;
    while at < span.byte_end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b'{' | b';' if parens == 0 && brackets == 0 && angles == 0 => return at,
            b'=' if parens == 0
                && brackets == 0
                && angles == 0
                && bytes.get(at + 1) == Some(&b'>') =>
            {
                return at;
            }
            _ => {}
        }
        at += 1;
    }
    span.byte_end.min(bytes.len())
}

fn top_level_assignment(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b'=' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                return Some(at);
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn top_level_segments(source: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut segments = Vec::new();
    let mut segment_start = start;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                segments.push((segment_start, at));
                segment_start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    segments.push((segment_start, end));
    segments
}

fn last_top_level_identifier(
    source: &str,
    start: usize,
    end: usize,
) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    let mut last = None;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    let mut at = start;
    while at < end.min(bytes.len()) {
        if is_identifier_start(bytes[at]) {
            let token = identifier_at(source, at).expect("identifier token");
            if parens == 0 && brackets == 0 && braces == 0 && angles == 0 {
                last = Some(token);
            }
            at = token.end;
            continue;
        }
        match bytes[at] {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            _ => {}
        }
        at += 1;
    }
    last
}

fn contains_receiver_formal(source: &str, start: usize, end: usize) -> bool {
    source
        .get(start..end)
        .is_some_and(|value| value.contains("this.") || value.contains("super."))
}

fn trim_range(source: &str, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    while end > start
        && bytes
            .get(end - 1)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b';')
    {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

fn find_identifier_named<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    name: &str,
) -> Option<IdentifierToken<'source>> {
    let mut at = start;
    while let Some(token) = next_identifier(source, at, end) {
        if token.text == name {
            return Some(token);
        }
        at = token.end;
    }
    None
}

fn next_identifier(source: &str, mut at: usize, end: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    while at < end.min(bytes.len()) && !is_identifier_start(bytes[at]) {
        at += 1;
    }
    identifier_at(source, at).filter(|token| token.end <= end)
}

fn find_next_byte(source: &str, mut at: usize, end: usize, target: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    while at < end.min(bytes.len()) {
        if bytes[at] == target {
            return Some(at);
        }
        at += 1;
    }
    None
}

fn matching_delimiter(
    source: &str,
    open: usize,
    end: usize,
    open_byte: u8,
    close_byte: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&open_byte) {
        return None;
    }
    let mut depth = 1usize;
    let mut at = open + 1;
    while at < end.min(bytes.len()) {
        if bytes[at] == open_byte {
            depth += 1;
        } else if bytes[at] == close_byte {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}

fn identifier_at(source: &str, start: usize) -> Option<IdentifierToken<'_>> {
    let bytes = source.as_bytes();
    if !bytes
        .get(start)
        .is_some_and(|byte| is_identifier_start(*byte))
    {
        return None;
    }
    let end = identifier_end(bytes, start);
    Some(IdentifierToken {
        text: &source[start..end],
        start,
        end,
    })
}

fn identifier_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn is_declaration_modifier(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "augment"
            | "const"
            | "covariant"
            | "external"
            | "final"
            | "late"
            | "required"
            | "static"
    )
}

fn is_non_nominal_root(value: &str) -> bool {
    matches!(
        value,
        "var" | "void" | "dynamic" | "get" | "set" | "operator" | "this" | "super"
    )
}

fn is_sdk_root(value: &str) -> bool {
    matches!(
        value,
        "BigInt"
            | "bool"
            | "DateTime"
            | "double"
            | "Duration"
            | "Function"
            | "Future"
            | "int"
            | "Iterable"
            | "List"
            | "Map"
            | "Never"
            | "Null"
            | "num"
            | "Object"
            | "Pattern"
            | "Record"
            | "RegExp"
            | "Set"
            | "StackTrace"
            | "Stream"
            | "String"
            | "Symbol"
            | "Type"
            | "Uri"
    )
}

fn supports_return_type(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Operator
    )
}

fn supports_parameters(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

fn supports_variable_type(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Variable
            | DartDeclarationKind::Field
            | DartDeclarationKind::LocalVariable
    )
}

fn is_type_declaration(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
