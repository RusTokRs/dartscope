use std::collections::HashMap;

use dartscope_core::DartLexicalBindingKind;

use crate::declaration_tables::DeclarationTables;
use crate::source_structure::SourceStructure;

use super::scan::{
    contains_top_level_pattern_start, find_keyword, find_top_level_keyword, has_top_level_byte,
    identifier_at, is_binding_name, top_level_assignment, top_level_byte_positions,
    top_level_identifiers, top_level_segments, trim_range,
};
use super::{LexicalRegionAnalysis, binding_for_token, write_for_token};

pub(super) fn collect_for_regions(
    source: &str,
    structure: &SourceStructure,
    tables: &DeclarationTables<'_>,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut ends = StatementEnds::new(source, structure);
    let mut search = 0usize;
    while let Some(found) = find_keyword(source, "for", search) {
        search = found + "for".len();
        let Some(open) = next_non_trivia(source, search) else {
            continue;
        };
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = structure.closing_paren(open) else {
            continue;
        };
        let Some(body_start) = next_non_trivia(source, close + 1) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        let Some((scope_start, scope_end, region_end)) =
            for_body_region(source, structure, &mut ends, body_start)
        else {
            result
                .deferred_regions
                .push((found, ends.end(body_start).unwrap_or(bytes.len())));
            continue;
        };
        if tables.has_local_declaration_starting_in(scope_start, scope_end) {
            result.deferred_regions.push((found, region_end));
            continue;
        }
        let Some(owner_id) = tables.innermost_callable_symbol(found) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        match parse_for_header(
            source,
            open + 1,
            close,
            scope_start,
            scope_end,
            owner_id,
            (&mut result.suppressed_regions, &mut result.write_targets),
        ) {
            Some(bindings) => result.bindings.extend(bindings),
            None => result.deferred_regions.push((found, region_end)),
        }
    }
}

fn for_body_region(
    source: &str,
    structure: &SourceStructure,
    ends: &mut StatementEnds<'_>,
    body_start: usize,
) -> Option<(usize, usize, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(body_start) == Some(&b'{') {
        let body_close = structure.closing_brace(body_start)?;
        return Some((body_start + 1, body_close, body_close + 1));
    }
    let body_end = ends.end(body_start)?;
    Some((body_start, body_end, body_end))
}

fn next_non_trivia(source: &str, mut at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'/') {
            at += 2;
            while bytes.get(at).is_some_and(|byte| *byte != b'\n') {
                at += 1;
            }
            continue;
        }
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'*') {
            at = block_comment_end(bytes, at)?;
            continue;
        }
        return (at < bytes.len()).then_some(at);
    }
}

fn block_comment_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'/') || bytes.get(start + 1) != Some(&b'*') {
        return None;
    }
    let mut depth = 1usize;
    let mut at = start + 2;
    while at < bytes.len() {
        if bytes.get(at) == Some(&b'/') && bytes.get(at + 1) == Some(&b'*') {
            depth += 1;
            at += 2;
            continue;
        }
        if bytes.get(at) == Some(&b'*') && bytes.get(at + 1) == Some(&b'/') {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return Some(at);
            }
            continue;
        }
        at += 1;
    }
    None
}

/// Where the statements of a text end, remembered by the offset they start at.
///
/// A statement such as `for (...) for (...) ... g();` ends where the statement inside it ends, so
/// measuring every loop of a long nest from its own start walks the rest of the nest each time, and
/// following the nest by recursion overflows the stack when it is thousands of levels deep. This
/// walk keeps its own stack of what is left to do with the end it finds, and remembers the end of
/// every statement it passed, so each statement is measured once.
struct StatementEnds<'a> {
    source: &'a str,
    structure: &'a SourceStructure,
    known: HashMap<usize, Option<usize>>,
}

/// What the measurement of a statement needs next.
enum Step {
    /// The statement ends here, or nowhere.
    Done(Option<usize>),
    /// The statement ends where the statement at this offset ends.
    Tail(usize),
    /// An `if`: the statement ends where the branch at this offset ends, unless an `else` follows.
    Then(usize),
    /// A `do`: the statement ends after the `while (...);` that follows the body at this offset.
    Body(usize),
}

/// What is left to do with the end of the statement that is being measured.
enum Pending {
    /// Remember it as the end of the statement that starts here.
    Remember(usize),
    /// It ends the branch of an `if`: look for an `else`.
    Else,
    /// It ends the body of a `do`: look for the `while (...);`.
    While,
}

impl<'a> StatementEnds<'a> {
    fn new(source: &'a str, structure: &'a SourceStructure) -> Self {
        Self {
            source,
            structure,
            known: HashMap::new(),
        }
    }

    /// The end of the statement that starts at `start`, after any trivia; `None` when it has none.
    fn end(&mut self, start: usize) -> Option<usize> {
        let mut pending = Vec::new();
        let mut at = start;
        'measure: loop {
            let mut value = loop {
                let Some(statement) = next_non_trivia(self.source, at) else {
                    break None;
                };
                if let Some(&known) = self.known.get(&statement) {
                    break known;
                }
                pending.push(Pending::Remember(statement));
                match self.step(statement) {
                    Step::Done(value) => break value,
                    Step::Tail(next) => at = next,
                    Step::Then(next) => {
                        pending.push(Pending::Else);
                        at = next;
                    }
                    Step::Body(next) => {
                        pending.push(Pending::While);
                        at = next;
                    }
                }
            };
            while let Some(frame) = pending.pop() {
                match frame {
                    Pending::Remember(statement) => {
                        self.known.insert(statement, value);
                    }
                    Pending::Else => {
                        let Some(then_end) = value else {
                            continue;
                        };
                        match self.else_keyword_end(then_end) {
                            Some(next) => {
                                at = next;
                                continue 'measure;
                            }
                            None => value = Some(then_end),
                        }
                    }
                    Pending::While => value = value.and_then(|end| self.do_end(end)),
                }
            }
            return value;
        }
    }

    /// Looks at the statement that starts at `start`, which is not trivia.
    fn step(&self, start: usize) -> Step {
        let source = self.source;
        let structure = self.structure;
        if source.as_bytes().get(start) == Some(&b'{') {
            return Step::Done(braced_statement_end(source, structure, start));
        }
        let Some(token) = identifier_at(source, start) else {
            return Step::Done(terminated_statement_end(source, start));
        };
        if is_label(source, token) {
            return match next_non_trivia(source, token.end) {
                Some(colon) => Step::Tail(colon + 1),
                None => Step::Done(None),
            };
        }
        match token.text {
            "if" => after_header(source, structure, token.end).map_or(Step::Done(None), Step::Then),
            "for" | "while" | "switch" => {
                after_header(source, structure, token.end).map_or(Step::Done(None), Step::Tail)
            }
            "await" if is_await_for(source, token) => next_non_trivia(source, token.end)
                .and_then(|for_start| identifier_at(source, for_start))
                .and_then(|for_token| after_header(source, structure, for_token.end))
                .map_or(Step::Done(None), Step::Tail),
            "do" => Step::Body(token.end),
            "try" => Step::Done(try_statement_end(source, structure, token.end)),
            _ => Step::Done(terminated_statement_end(source, start)),
        }
    }

    /// The end of the `else` keyword that follows the branch ending at `then_end`, if any.
    fn else_keyword_end(&self, then_end: usize) -> Option<usize> {
        let else_start = next_non_trivia(self.source, then_end)?;
        let else_token = identifier_at(self.source, else_start)?;
        (else_token.text == "else").then_some(else_token.end)
    }

    /// The end of a `do` statement whose body ends at `body_end`.
    fn do_end(&self, body_end: usize) -> Option<usize> {
        let source = self.source;
        let while_start = next_non_trivia(source, body_end)?;
        let while_token = identifier_at(source, while_start)?;
        if while_token.text != "while" {
            return None;
        }
        let open = next_non_trivia(source, while_token.end)?;
        let close = self.structure.closing_paren(open)?;
        let semicolon = next_non_trivia(source, close + 1)?;
        (source.as_bytes().get(semicolon) == Some(&b';')).then_some(semicolon + 1)
    }
}

/// The offset after the parenthesized header that follows the keyword ending at `keyword_end`.
fn after_header(source: &str, structure: &SourceStructure, keyword_end: usize) -> Option<usize> {
    let open = next_non_trivia(source, keyword_end)?;
    if source.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    structure.closing_paren(open).map(|close| close + 1)
}

fn try_statement_end(
    source: &str,
    structure: &SourceStructure,
    keyword_end: usize,
) -> Option<usize> {
    let mut end = braced_statement_end(source, structure, keyword_end)?;
    let mut saw_handler = false;
    loop {
        let Some(clause_start) = next_non_trivia(source, end) else {
            return saw_handler.then_some(end);
        };
        let Some(clause) = identifier_at(source, clause_start) else {
            return saw_handler.then_some(end);
        };
        match clause.text {
            "on" => {
                end = on_clause_end(source, structure, clause.end)?;
                saw_handler = true;
            }
            "catch" => {
                end = catch_clause_end(source, structure, clause.end)?;
                saw_handler = true;
            }
            "finally" => return braced_statement_end(source, structure, clause.end),
            _ => return saw_handler.then_some(end),
        }
    }
}

fn on_clause_end(source: &str, structure: &SourceStructure, keyword_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut at = next_non_trivia(source, keyword_end)?;
    while at < bytes.len() {
        if parens == 0 && brackets == 0 {
            if bytes[at] == b'{' {
                return braced_statement_end(source, structure, at);
            }
            if matches!(bytes[at], b';' | b'}') {
                return None;
            }
            if let Some(token) = identifier_at(source, at) {
                if token.text == "catch" {
                    return catch_clause_end(source, structure, token.end);
                }
                at = token.end;
                continue;
            }
        }
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => return None,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => return None,
            b']' => brackets -= 1,
            _ => {}
        }
        at += 1;
    }
    None
}

fn catch_clause_end(
    source: &str,
    structure: &SourceStructure,
    keyword_end: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let open = next_non_trivia(source, keyword_end)?;
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let close = structure.closing_paren(open)?;
    braced_statement_end(source, structure, close + 1)
}

fn braced_statement_end(source: &str, structure: &SourceStructure, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let open = next_non_trivia(source, start)?;
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    structure.closing_brace(open).map(|end| end + 1)
}

fn terminated_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'(' => parens += 1,
            b')' if parens == 0 => return None,
            b')' => parens -= 1,
            b'[' => brackets += 1,
            b']' if brackets == 0 => return None,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' if braces == 0 => return None,
            b'}' => braces -= 1,
            b';' if parens == 0 && brackets == 0 && braces == 0 => return Some(at + 1),
            _ => {}
        }
        at += 1;
    }
    None
}

fn is_await_for(source: &str, token: super::IdentifierToken<'_>) -> bool {
    if token.text != "await" {
        return false;
    }
    next_non_trivia(source, token.end)
        .and_then(|start| identifier_at(source, start))
        .is_some_and(|next| next.text == "for")
}

fn is_label(source: &str, token: super::IdentifierToken<'_>) -> bool {
    next_non_trivia(source, token.end).is_some_and(|at| source.as_bytes().get(at) == Some(&b':'))
}

#[derive(Debug, Clone, Copy)]
struct ClassicForDeclarator<'source> {
    token: super::IdentifierToken<'source>,
    declaration_start: usize,
    declaration_end: usize,
    scope_start: usize,
}

#[derive(Debug)]
enum ClassicForInitializer<'source> {
    Expression,
    Declaration(Vec<ClassicForDeclarator<'source>>),
}

fn parse_for_header(
    source: &str,
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    owner_id: &str,
    outputs: (
        &mut Vec<(usize, usize)>,
        &mut Vec<super::LexicalRegionWrite>,
    ),
) -> Option<Vec<super::LexicalRegionBinding>> {
    let semicolons = top_level_byte_positions(source, start, end, b';');
    if semicolons.is_empty() {
        return parse_for_in_header(source, start, end, body_start, body_end, owner_id, outputs);
    }
    if semicolons.len() != 2 {
        return None;
    }
    let Some((init_start, init_end)) = trim_range(source, start, semicolons[0]) else {
        return Some(Vec::new());
    };
    let initializer = parse_classic_for_initializer(source, init_start, init_end)?;
    let ClassicForInitializer::Declaration(declarators) = initializer else {
        return Some(Vec::new());
    };
    let bindings = declarators
        .iter()
        .map(|declarator| {
            binding_for_token(
                declarator.token,
                DartLexicalBindingKind::LocalVariable,
                "for_variable",
                declarator.scope_start,
                body_end,
                owner_id,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    outputs.0.extend(
        declarators
            .iter()
            .map(|declarator| (declarator.declaration_start, declarator.declaration_end)),
    );
    Some(bindings)
}

fn parse_classic_for_initializer<'source>(
    source: &'source str,
    start: usize,
    end: usize,
) -> Option<ClassicForInitializer<'source>> {
    let segments = top_level_segments(source, start, end, b',');
    let (first_start, first_end) = *segments.first()?;
    let (first_start, first_end) = trim_range(source, first_start, first_end)?;
    let declaration_end = top_level_assignment(source, first_start, first_end).unwrap_or(first_end);
    if contains_top_level_pattern_start(source, first_start, declaration_end) {
        return None;
    }
    let tokens = top_level_identifiers(source, first_start, declaration_end);
    let declares = !tokens.is_empty()
        && !source[first_start..declaration_end].contains('.')
        && (is_declaration_prefix(tokens[0].text) || tokens.len() >= 2);
    if !declares {
        return (segments.len() == 1
            && !contains_top_level_pattern_start(source, first_start, first_end))
        .then_some(ClassicForInitializer::Expression);
    }

    let mut declarators = Vec::with_capacity(segments.len());
    declarators.push(parse_classic_for_declarator(
        source,
        first_start,
        first_end,
        true,
    )?);
    for (segment_start, segment_end) in segments.into_iter().skip(1) {
        declarators.push(parse_classic_for_declarator(
            source,
            segment_start,
            segment_end,
            false,
        )?);
    }
    Some(ClassicForInitializer::Declaration(declarators))
}

fn parse_classic_for_declarator<'source>(
    source: &'source str,
    start: usize,
    end: usize,
    first: bool,
) -> Option<ClassicForDeclarator<'source>> {
    let (start, end) = trim_range(source, start, end)?;
    let assignment = top_level_assignment(source, start, end);
    let declaration_end = assignment.unwrap_or(end);
    let (declaration_start, declaration_name_end) = trim_range(source, start, declaration_end)?;
    if contains_top_level_pattern_start(source, declaration_start, declaration_name_end)
        || source[declaration_start..declaration_name_end].contains('.')
    {
        return None;
    }
    let tokens = top_level_identifiers(source, declaration_start, declaration_name_end);
    let token = *tokens.last()?;
    if !is_binding_name(token.text) {
        return None;
    }
    if !first
        && (tokens.len() != 1
            || token.start != declaration_start
            || token.end != declaration_name_end)
    {
        return None;
    }
    let scope_start = match assignment {
        Some(assignment) => trim_range(source, assignment + 1, end)?.1,
        None => token.end,
    };
    Some(ClassicForDeclarator {
        token,
        declaration_start: start,
        declaration_end,
        scope_start,
    })
}

fn parse_for_in_header(
    source: &str,
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    owner_id: &str,
    outputs: (
        &mut Vec<(usize, usize)>,
        &mut Vec<super::LexicalRegionWrite>,
    ),
) -> Option<Vec<super::LexicalRegionBinding>> {
    let in_at = find_top_level_keyword(source, start, end, "in")?;
    let (left_start, left_end) = trim_range(source, start, in_at)?;
    if has_top_level_byte(source, left_start, left_end, b',')
        || contains_top_level_pattern_start(source, left_start, left_end)
        || source[left_start..left_end].contains('.')
    {
        return None;
    }
    let tokens = top_level_identifiers(source, left_start, left_end);
    if tokens.is_empty() {
        return None;
    }
    let declares = is_declaration_prefix(tokens[0].text) || tokens.len() >= 2;
    if !declares {
        let target = *tokens.first()?;
        if tokens.len() != 1
            || target.start != left_start
            || target.end != left_end
            || !is_binding_name(target.text)
        {
            return None;
        }
        outputs.0.push((left_start, left_end));
        outputs.1.push(write_for_token(target, owner_id)?);
        return Some(Vec::new());
    }
    let name = *tokens.last()?;
    if !is_binding_name(name.text) {
        return None;
    }
    outputs.0.push((left_start, left_end));
    binding_for_token(
        name,
        DartLexicalBindingKind::LocalVariable,
        "for_variable",
        body_start,
        body_end,
        owner_id,
    )
    .map(|binding| vec![binding])
}

pub(super) fn collect_catch_regions(
    source: &str,
    structure: &SourceStructure,
    tables: &DeclarationTables<'_>,
    result: &mut LexicalRegionAnalysis,
) {
    let bytes = source.as_bytes();
    let mut ends = StatementEnds::new(source, structure);
    let mut search = 0usize;
    while let Some(found) = find_keyword(source, "catch", search) {
        search = found + "catch".len();
        let Some(open) = next_non_trivia(source, search) else {
            continue;
        };
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = structure.closing_paren(open) else {
            continue;
        };
        let Some(body_open) = next_non_trivia(source, close + 1) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        if bytes.get(body_open) != Some(&b'{') {
            result
                .deferred_regions
                .push((found, ends.end(body_open).unwrap_or(bytes.len())));
            continue;
        }
        let Some(body_close) = structure.closing_brace(body_open) else {
            result.deferred_regions.push((found, bytes.len()));
            continue;
        };
        let region_end = body_close + 1;
        let Some(owner_id) = tables.innermost_callable_symbol(found) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        let Some(tokens) = simple_identifier_segments(source, open + 1, close, 2) else {
            result.deferred_regions.push((found, region_end));
            continue;
        };
        result.suppressed_regions.push((open + 1, close));
        for token in tokens {
            if let Some(binding) = binding_for_token(
                token,
                DartLexicalBindingKind::LocalVariable,
                "catch_parameter",
                body_open + 1,
                body_close,
                owner_id,
            ) {
                result.bindings.push(binding);
            }
        }
    }
}

fn simple_identifier_segments(
    source: &str,
    start: usize,
    end: usize,
    max_segments: usize,
) -> Option<Vec<super::IdentifierToken<'_>>> {
    let segments = super::scan::top_level_segments(source, start, end, b',');
    if segments.is_empty() || segments.len() > max_segments {
        return None;
    }
    let mut tokens = Vec::new();
    for (segment_start, segment_end) in segments {
        let (segment_start, segment_end) = trim_range(source, segment_start, segment_end)?;
        let token = identifier_at(source, segment_start)?;
        if token.end != segment_end {
            return None;
        }
        if is_binding_name(token.text) {
            tokens.push(token);
        }
    }
    Some(tokens)
}

fn is_declaration_prefix(value: &str) -> bool {
    matches!(value, "var" | "final" | "const" | "late")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The recursive measurement that `StatementEnds` replaces, kept as the specification.
    fn oracle_statement_end(source: &str, structure: &SourceStructure, start: usize) -> Option<usize> {
        let bytes = source.as_bytes();
        let start = next_non_trivia(source, start)?;
        if bytes.get(start) == Some(&b'{') {
            return braced_statement_end(source, structure, start);
        }
        let Some(token) = identifier_at(source, start) else {
            return terminated_statement_end(source, start);
        };
        if is_label(source, token) {
            let colon = next_non_trivia(source, token.end)?;
            return oracle_statement_end(source, structure, colon + 1);
        }
        match token.text {
            "if" => oracle_if_statement_end(source, structure, token.end),
            "for" | "while" | "switch" => oracle_header_statement_end(source, structure, token.end),
            "await" if is_await_for(source, token) => {
                let for_start = next_non_trivia(source, token.end)?;
                let for_token = identifier_at(source, for_start)?;
                oracle_header_statement_end(source, structure, for_token.end)
            }
            "do" => oracle_do_statement_end(source, structure, token.end),
            "try" => try_statement_end(source, structure, token.end),
            _ => terminated_statement_end(source, start),
        }
    }

    fn oracle_if_statement_end(
        source: &str,
        structure: &SourceStructure,
        keyword_end: usize,
    ) -> Option<usize> {
        let then_end = oracle_header_statement_end(source, structure, keyword_end)?;
        let Some(else_start) = next_non_trivia(source, then_end) else {
            return Some(then_end);
        };
        let Some(else_token) = identifier_at(source, else_start) else {
            return Some(then_end);
        };
        if else_token.text != "else" {
            return Some(then_end);
        }
        oracle_statement_end(source, structure, else_token.end)
    }

    fn oracle_header_statement_end(
        source: &str,
        structure: &SourceStructure,
        keyword_end: usize,
    ) -> Option<usize> {
        let bytes = source.as_bytes();
        let open = next_non_trivia(source, keyword_end)?;
        if bytes.get(open) != Some(&b'(') {
            return None;
        }
        let close = structure.closing_paren(open)?;
        oracle_statement_end(source, structure, close + 1)
    }

    fn oracle_do_statement_end(
        source: &str,
        structure: &SourceStructure,
        keyword_end: usize,
    ) -> Option<usize> {
        let bytes = source.as_bytes();
        let body_end = oracle_statement_end(source, structure, keyword_end)?;
        let while_start = next_non_trivia(source, body_end)?;
        let while_token = identifier_at(source, while_start)?;
        if while_token.text != "while" {
            return None;
        }
        let open = next_non_trivia(source, while_token.end)?;
        let close = structure.closing_paren(open)?;
        let semicolon = next_non_trivia(source, close + 1)?;
        (bytes.get(semicolon) == Some(&b';')).then_some(semicolon + 1)
    }

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
            usize::try_from(value % bound as u64).unwrap_or(0)
        }
    }

    const TOKENS: &[&str] = &[
        "for (a; b; c) ",
        "for (x in y) ",
        "await for (x in y) ",
        "while (x) ",
        "switch (v) ",
        "if (x) ",
        "else ",
        "do ",
        "while (y);",
        "try ",
        "on T ",
        "catch (e) ",
        "finally ",
        "{ ",
        "} ",
        "( ",
        ") ",
        "[ ",
        "] ",
        "g(); ",
        "x = 1; ",
        "label: ",
        "; ",
        "\n",
        "// c\n",
        "/* c */ ",
    ];

    #[test]
    fn the_walk_measures_every_statement_like_the_recursion() {
        let mut rng = Rng(0x5EED_CAFE_F00D_0001);
        let mut checked = 0usize;
        for round in 0..3000 {
            let count = 1 + round % 14;
            let source: String = (0..count)
                .map(|_| TOKENS[rng.below(TOKENS.len())])
                .collect();
            let structure = SourceStructure::new(&source);
            // One walk answers every question about the text, in an order that makes it reuse
            // what it remembered in both directions.
            let mut ends = StatementEnds::new(&source, &structure);
            let mut starts: Vec<usize> = (0..=source.len() + 1).collect();
            if round % 2 == 1 {
                starts.reverse();
            }
            for start in starts {
                assert_eq!(
                    ends.end(start),
                    oracle_statement_end(&source, &structure, start),
                    "{source:?} from {start}"
                );
                checked += 1;
            }
        }
        assert!(checked > 30_000, "{checked} questions were asked");
    }

    #[test]
    fn a_nest_of_thousands_of_loops_is_measured_in_one_walk() {
        // The recursion would need a stack frame for each of the 100,000 levels, and measuring each
        // loop from its own start would visit about 5 * 10^9 of them.
        let depth = 100_000;
        let source = format!("{}g();", "for (var i = 0; i < 1; i++) ".repeat(depth));
        let structure = SourceStructure::new(&source);
        let mut ends = StatementEnds::new(&source, &structure);
        let mut at = 0;
        let mut measured = 0;
        while let Some(found) = find_keyword(&source, "for", at) {
            at = found + 3;
            assert_eq!(ends.end(found), Some(source.len()));
            measured += 1;
        }
        assert_eq!(measured, depth);
        // One entry for every loop and one for the statement inside the innermost.
        assert_eq!(ends.known.len(), depth + 1);
    }
}
