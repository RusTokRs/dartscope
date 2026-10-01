//! Normalized declaration inventory for the conservative parser backend.

mod scanner;
mod syntax;

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartDiagnostic};

use self::scanner::{
    AnnotationRuns, BraceDepths, EndMode, STATEMENT_PROBE_BYTES, body_range, declaration_end,
    declaration_header, declaration_header_within, enum_member_start, first_code_byte,
    next_code_byte,
};
use self::syntax::{
    SymbolIdAllocator, callable_end_mode, enum_constants, has_primary_constructor,
    is_callable_kind, is_concise_constructor, is_directive, is_type_kind, kind_label,
    local_variable_names, member_headers, top_level_accessor, top_level_variables, type_header,
    type_relations,
};
use crate::declarations::top_level_function;
use crate::source_lines::{line_span, line_span_for_byte, source_lines, span_for_byte_range};

#[derive(Debug, Clone)]
struct DeclarationRecord {
    declaration: DartDeclaration,
    body: Option<(usize, usize)>,
}

pub(crate) fn collect_declaration_inventory(
    path: &str,
    source: &str,
    masked_source: &str,
) -> (Vec<DartDeclaration>, Vec<DartDiagnostic>) {
    let lines = source_lines(masked_source);
    let depths = BraceDepths::new(masked_source);
    let mut diagnostics = Vec::new();
    let mut records = collect_top_level(
        path,
        source,
        masked_source,
        &lines,
        &depths,
        &mut diagnostics,
    );

    let type_records: Vec<_> = records
        .iter()
        .filter(|record| is_type_kind(record.declaration.kind))
        .cloned()
        .collect();
    for type_record in type_records {
        collect_members(
            source,
            masked_source,
            &lines,
            &depths,
            &type_record,
            &mut records,
            &mut diagnostics,
        );
    }

    let callable_records: Vec<_> = records
        .iter()
        .filter(|record| is_callable_kind(record.declaration.kind))
        .cloned()
        .collect();
    for callable in callable_records {
        collect_locals(source, masked_source, &lines, &callable, &mut records);
    }

    records.sort_by(|left, right| {
        left.declaration
            .declaration_span
            .as_ref()
            .map(|span| span.byte_start)
            .cmp(
                &right
                    .declaration
                    .declaration_span
                    .as_ref()
                    .map(|span| span.byte_start),
            )
            .then_with(|| left.declaration.kind.cmp(&right.declaration.kind))
            .then_with(|| left.declaration.name.cmp(&right.declaration.name))
    });

    (
        records
            .into_iter()
            .map(|record| record.declaration)
            .collect(),
        diagnostics,
    )
}

fn collect_top_level(
    path: &str,
    source: &str,
    masked: &str,
    lines: &[crate::source_lines::SourceLine<'_>],
    depths: &BraceDepths,
    diagnostics: &mut Vec<DartDiagnostic>,
) -> Vec<DeclarationRecord> {
    let mut records = Vec::new();
    let mut cursor = 0usize;
    let mut ids = SymbolIdAllocator::default();
    let mut annotations = AnnotationRuns::new(masked.len());

    for line in lines.iter().copied() {
        if line.byte_end() <= cursor {
            continue;
        }
        let line_start = first_code_byte(line, masked).max(cursor);
        // Indentation only distinguishes a declaration from an expression continuation when the scan
        // starts at the beginning of the line; a declaration that follows another declaration on the
        // same line, or a continuation of a multi-line declaration, is treated as unindented.
        let mut from_line_start = line_start == first_code_byte(line, masked);
        let line_indent = line
            .text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();
        let mut at = line_start;
        while at < line.byte_end() {
            if depths.at(at) != 0 {
                break;
            }
            // An annotation tail can share the declaration's line, and a declaration may be indented
            // only when it starts the line it is written on.
            if skip_surplus_closer(masked, line, &mut at) {
                continue;
            }
            let declared_at = annotations.end(masked, at);
            if declared_at >= line.byte_end() {
                break;
            }
            let indent = if from_line_start { line_indent } else { 0 };
            let Some((mut found, end)) = top_level_records(
                path,
                source,
                masked,
                line,
                indent,
                &mut ids,
                diagnostics,
                declared_at,
            ) else {
                break;
            };
            records.append(&mut found);
            cursor = end;
            from_line_start = false;
            match next_code_byte(masked, cursor, line.byte_end()) {
                Some(next) => at = next,
                None => break,
            }
        }
    }

    records
}

/// Advances `at` past a closing delimiter that cannot start a declaration but may end an annotation or
/// a multi-line declaration header on the line being scanned. Returns `true` when `at` moved.
fn skip_surplus_closer(
    masked: &str,
    line: crate::source_lines::SourceLine<'_>,
    at: &mut usize,
) -> bool {
    if !matches!(masked.as_bytes()[*at], b')' | b']') {
        return false;
    }
    match next_code_byte(masked, *at + 1, line.byte_end()) {
        Some(next) => {
            *at = next;
            true
        }
        None => {
            *at = line.byte_end();
            true
        }
    }
}

/// Collects every top-level declaration that starts at `at`, which may sit in the middle of a source
/// line. Returns the declarations and the byte offset where scanning may continue.
#[allow(clippy::too_many_arguments)]
fn top_level_records(
    path: &str,
    source: &str,
    masked: &str,
    line: crate::source_lines::SourceLine<'_>,
    indent: usize,
    ids: &mut SymbolIdAllocator,
    diagnostics: &mut Vec<DartDiagnostic>,
    at: usize,
) -> Option<(Vec<DeclarationRecord>, usize)> {
    let header = declaration_header(masked, at)?;
    if is_directive(header) {
        let end = declaration_end(masked, at, EndMode::SemicolonOnly).unwrap_or(line.byte_end());
        return Some((Vec::new(), end));
    }
    if header.trim_start().starts_with('@') {
        return None;
    }
    let anchor = line_span(source, line);

    if let Some((name, kind)) = type_header(header) {
        let end = declaration_end(masked, at, EndMode::BodyOrSemicolon).unwrap_or(line.byte_end());
        let symbol_id = ids.allocate(format!("{path}::{}:{name}", kind_label(kind)));
        let body = body_range(masked, at, end);
        let full_span = span_for_byte_range(source, at, end);
        let relations = type_relations(header, kind);
        let declaration = DartDeclaration {
            name: name.clone(),
            kind,
            span: anchor.clone(),
            extends: relations.extends,
            mixes_in: relations.mixes_in,
            on_types: relations.on_types,
            symbol_id: Some(symbol_id),
            parent_symbol_id: None,
            declaration_span: Some(full_span),
        };
        if kind == DartDeclarationKind::Class && has_primary_constructor(header, &name) {
            diagnostics.push(DartDiagnostic::warning(
                "unsupported_primary_constructor",
                "primary constructor syntax requires a language-version-aware parser backend",
                Some(anchor),
            ));
        }
        return Some((vec![DeclarationRecord { declaration, body }], end));
    }

    let names = top_level_variables(header.trim(), indent);
    if !names.is_empty() {
        let end = declaration_end(masked, at, EndMode::SemicolonOnly).unwrap_or(line.byte_end());
        let full_span = span_for_byte_range(source, at, end);
        let records = names
            .into_iter()
            .map(|name| {
                let symbol_id = ids.allocate(format!("{path}::variable:{name}"));
                DeclarationRecord {
                    declaration: DartDeclaration {
                        name,
                        kind: DartDeclarationKind::Variable,
                        span: anchor.clone(),
                        extends: None,
                        mixes_in: Vec::new(),
                        on_types: Vec::new(),
                        symbol_id: Some(symbol_id),
                        parent_symbol_id: None,
                        declaration_span: Some(full_span.clone()),
                    },
                    body: None,
                }
            })
            .collect();
        return Some((records, end));
    }

    let (name, kind) = top_level_function(header.trim(), indent)
        .map(|name| (name, DartDeclarationKind::Function))
        .or_else(|| top_level_accessor(header.trim(), indent))?;
    let end = declaration_end(masked, at, callable_end_mode(header)).unwrap_or(line.byte_end());
    let symbol_id = ids.allocate(format!("{path}::{}:{name}", kind_label(kind)));
    let declaration = DartDeclaration {
        name,
        kind,
        span: anchor,
        extends: None,
        mixes_in: Vec::new(),
        on_types: Vec::new(),
        symbol_id: Some(symbol_id),
        parent_symbol_id: None,
        declaration_span: Some(span_for_byte_range(source, at, end)),
    };
    Some((
        vec![DeclarationRecord {
            declaration,
            body: body_range(masked, at, end),
        }],
        end,
    ))
}

#[allow(clippy::too_many_arguments)]
fn collect_members(
    source: &str,
    masked: &str,
    lines: &[crate::source_lines::SourceLine<'_>],
    depths: &BraceDepths,
    owner: &DeclarationRecord,
    records: &mut Vec<DeclarationRecord>,
    diagnostics: &mut Vec<DartDiagnostic>,
) {
    let Some((body_start, body_end)) = owner.body else {
        return;
    };
    let owner_id = owner.declaration.symbol_id.as_deref().unwrap_or_default();
    let owner_depth = depths.at(body_start) + 1;
    let member_start = if owner.declaration.kind == DartDeclarationKind::Enum {
        enum_member_start(masked, body_start, body_end, owner_depth).unwrap_or(body_end)
    } else {
        body_start + 1
    };
    let mut cursor = member_start;
    let mut ids = SymbolIdAllocator::default();
    let mut annotations = AnnotationRuns::new(body_end);

    if owner.declaration.kind == DartDeclarationKind::Enum {
        for constant in enum_constants(masked, body_start, body_end) {
            let symbol_id = ids.allocate(format!("{owner_id}/field:{}", constant.name));
            records.push(DeclarationRecord {
                declaration: DartDeclaration {
                    name: constant.name,
                    kind: DartDeclarationKind::Field,
                    span: line_span_for_byte(source, constant.start),
                    extends: None,
                    mixes_in: Vec::new(),
                    on_types: Vec::new(),
                    symbol_id: Some(symbol_id),
                    parent_symbol_id: Some(owner_id.to_string()),
                    declaration_span: Some(span_for_byte_range(
                        source,
                        constant.start,
                        constant.end,
                    )),
                },
                body: None,
            });
        }
    }

    // Lines that end before the first member cannot contain one; skipping them with a binary
    // search keeps the total work linear when a file declares many types.
    let first_line = lines.partition_point(|line| line.byte_end() <= cursor);
    for line in lines.iter().copied().skip(first_line) {
        #[cfg(test)]
        note_visited_line();
        // Lines are in order, so once one starts at the end of the body no later line can hold a
        // member; without this check the scan of every type walks on through the rest of the file.
        if cursor >= body_end || line.byte_start >= body_end {
            break;
        }
        if line.byte_end() <= cursor {
            continue;
        }
        let mut at = first_code_byte(line, masked).max(cursor);
        while at < body_end && at < line.byte_end() {
            if depths.at(at) != owner_depth {
                break;
            }
            if masked.as_bytes()[at] == b'}' {
                break;
            }
            if skip_surplus_closer(masked, line, &mut at) {
                continue;
            }
            let declared_at = annotations.end(masked, at);
            if declared_at >= line.byte_end() {
                break;
            }
            let Some(header) = declaration_header(masked, declared_at) else {
                break;
            };
            let declaration_at = declared_at;
            if declaration_at + header.len() > body_end {
                break;
            }
            let header = header.trim();
            if header.is_empty() || header.starts_with('@') || header.starts_with("case ") {
                break;
            }
            if is_concise_constructor(header, &owner.declaration.name) {
                diagnostics.push(DartDiagnostic::warning(
                    "unsupported_concise_constructor",
                    "concise constructor syntax requires Dart 3.13 language-version handling",
                    Some(line_span(source, line)),
                ));
                cursor = declaration_end(masked, declaration_at, EndMode::BodyOrSemicolon)
                    .unwrap_or(line.byte_end());
                match next_code_byte(masked, cursor, line.byte_end()) {
                    Some(next) => at = next,
                    None => break,
                }
                continue;
            }

            let members = member_headers(header, &owner.declaration.name);
            let Some((_, _, mode)) = members.first() else {
                break;
            };
            let end = declaration_end(masked, declaration_at, *mode).unwrap_or(line.byte_end());
            let full_span = span_for_byte_range(source, declaration_at, end);
            let body = body_range(masked, declaration_at, end);
            for (name, kind, _) in members {
                let base_id = format!("{owner_id}/{}:{name}", kind_label(kind));
                let symbol_id = ids.allocate(base_id);
                let declaration = DartDeclaration {
                    name,
                    kind,
                    span: line_span(source, line),
                    extends: None,
                    mixes_in: Vec::new(),
                    on_types: Vec::new(),
                    symbol_id: Some(symbol_id),
                    parent_symbol_id: Some(owner_id.to_string()),
                    declaration_span: Some(full_span.clone()),
                };
                records.push(DeclarationRecord { declaration, body });
            }
            cursor = end;
            match next_code_byte(masked, cursor, line.byte_end()) {
                Some(next) => at = next,
                None => break,
            }
        }
    }
}

fn collect_locals(
    source: &str,
    masked: &str,
    lines: &[crate::source_lines::SourceLine<'_>],
    owner: &DeclarationRecord,
    records: &mut Vec<DeclarationRecord>,
) {
    let Some((body_start, body_end)) = owner.body else {
        return;
    };
    let owner_id = owner.declaration.symbol_id.as_deref().unwrap_or_default();
    let mut cursor = body_start + 1;
    let mut ids = SymbolIdAllocator::default();
    let mut annotations = AnnotationRuns::new(body_end);

    let first_line = lines.partition_point(|line| line.byte_end() <= cursor);
    for line in lines.iter().copied().skip(first_line) {
        #[cfg(test)]
        note_visited_line();
        // See `collect_members`: a line that starts at the end of the body ends the scan.
        if cursor >= body_end || line.byte_start >= body_end {
            break;
        }
        if line.byte_end() <= cursor {
            continue;
        }
        let mut at = first_code_byte(line, masked).max(cursor);
        while at < body_end && at < line.byte_end() {
            if masked.as_bytes()[at] == b'}' {
                break;
            }
            if skip_surplus_closer(masked, line, &mut at) {
                continue;
            }
            let declared_at = annotations.end(masked, at);
            if declared_at >= line.byte_end() {
                break;
            }
            let Some((probe, complete)) =
                declaration_header_within(masked, declared_at, STATEMENT_PROBE_BYTES)
            else {
                break;
            };
            let header = if complete {
                probe
            } else {
                // A long statement is only worth scanning to its end when its beginning declares
                // something; every other line of a long call or literal would repeat that scan.
                if local_variable_names(probe.trim()).is_empty() {
                    break;
                }
                let Some(header) = declaration_header(masked, declared_at) else {
                    break;
                };
                header
            };
            let declaration_at = declared_at;
            if declaration_at + header.len() > body_end {
                break;
            }
            let names = local_variable_names(header.trim());
            if names.is_empty() {
                break;
            }
            let end = declaration_end(masked, declaration_at, EndMode::SemicolonOnly)
                .unwrap_or(line.byte_end());
            let full_span = span_for_byte_range(source, declaration_at, end);
            for name in names {
                let symbol_id = ids.allocate(format!("{owner_id}/local_variable:{name}"));
                records.push(DeclarationRecord {
                    declaration: DartDeclaration {
                        name,
                        kind: DartDeclarationKind::LocalVariable,
                        span: line_span(source, line),
                        extends: None,
                        mixes_in: Vec::new(),
                        on_types: Vec::new(),
                        symbol_id: Some(symbol_id),
                        parent_symbol_id: Some(owner_id.to_string()),
                        declaration_span: Some(full_span.clone()),
                    },
                    body: None,
                });
            }
            cursor = end;
            match next_code_byte(masked, cursor, line.byte_end()) {
                Some(next) => at = next,
                None => break,
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    static VISITED_LINES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts one line examined by a member or local scan (test builds only).
#[cfg(test)]
fn note_visited_line() {
    VISITED_LINES.with(|visited| visited.set(visited.get() + 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexical::mask_non_code;

    #[test]
    fn member_and_local_scans_stop_at_the_end_of_their_body() {
        let count = 1500;
        let mut source = String::new();
        for index in 0..count {
            source.push_str(&format!(
                "void f{index}() {{\n  var x = {index};\n}}\nclass C{index} {{\n  int m() {{\n    var y = 1;\n  }}\n}}\n"
            ));
        }
        let masked = mask_non_code(&source).code;
        let lines = source_lines(&masked).len();
        // Without a line table for the text every span would rebuild it.
        let _scope = crate::source_lines::LineIndexScope::enter(&source);

        let before = VISITED_LINES.with(std::cell::Cell::get);
        let (declarations, _) = collect_declaration_inventory("lib/a.dart", &source, &masked);
        let visited = VISITED_LINES.with(std::cell::Cell::get) - before;

        let locals = declarations
            .iter()
            .filter(|declaration| declaration.kind == DartDeclarationKind::LocalVariable)
            .count();
        assert_eq!(locals, 2 * count);
        // Every scan reads the lines of its own body and one more. Scanning on to the end of the
        // file from each body would visit about lines^2 / 2 of them.
        assert!(
            visited <= 3 * lines,
            "{visited} lines were visited for a file of {lines} lines"
        );
    }

    #[test]
    fn a_statement_with_thousands_of_argument_lines_is_not_rescanned_from_every_line() {
        let lines = 6000;
        let mut source = String::from("void f() {\n  g(\n");
        for index in 0..lines {
            source.push_str(&format!("    {index},\n"));
        }
        source.push_str("  );\n}\n");
        let masked = mask_non_code(&source).code;

        let before = scanner::scanned_header_bytes();
        let (declarations, _) = collect_declaration_inventory("lib/a.dart", &source, &masked);
        let scanned = scanner::scanned_header_bytes() - before;

        assert_eq!(
            declarations
                .iter()
                .map(|declaration| declaration.name.as_str())
                .collect::<Vec<_>>(),
            ["f"]
        );
        // Every line examines at most the probe, so the work is linear in the number of lines. A
        // scan from each line to the end of the statement would examine about lines^2 * 5 bytes.
        assert!(
            scanned <= (lines + 8) * (STATEMENT_PROBE_BYTES + 16),
            "{scanned} header bytes were scanned for {lines} argument lines"
        );
    }
}
