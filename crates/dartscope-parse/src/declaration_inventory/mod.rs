//! Normalized declaration inventory for the conservative parser backend.

mod scanner;
mod syntax;

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartDiagnostic, SourceSpan};

use self::scanner::{
    EndMode, annotations_end, body_range, brace_depth_at, declaration_end, declaration_header,
    depth_within_line, enum_member_start, first_code_byte, line_brace_depths, next_code_byte,
    source_line_text,
};
use self::syntax::{
    SymbolIdAllocator, has_primary_constructor, is_callable_kind, is_concise_constructor,
    is_directive, is_type_kind, kind_label, local_variable_names, member_headers,
    top_level_variables, type_header,
};
use crate::declarations::{top_level_function, value_after_keyword, values_after_keyword};
use crate::source_lines::{source_lines, span_for_byte_range};

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
    let line_depths = line_brace_depths(masked_source, &lines);
    let mut diagnostics = Vec::new();
    let mut records = collect_top_level(
        path,
        source,
        masked_source,
        &lines,
        &line_depths,
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
            &line_depths,
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
    line_depths: &[usize],
    diagnostics: &mut Vec<DartDiagnostic>,
) -> Vec<DeclarationRecord> {
    let mut records = Vec::new();
    let mut cursor = 0usize;
    let mut ids = SymbolIdAllocator::default();

    for (index, line) in lines.iter().copied().enumerate() {
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
            if depth_within_line(masked, line, line_depths[index], at) != 0 {
                break;
            }
            // An annotation tail can share the declaration's line, and a declaration may be indented
            // only when it starts the line it is written on.
            if skip_surplus_closer(masked, line, &mut at) {
                continue;
            }
            let declared_at = annotations_end(masked, at, masked.len());
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
    let anchor = SourceSpan::line(line.number, line.byte_start, source_line_text(source, line));

    if let Some((name, kind)) = type_header(header) {
        let end = declaration_end(masked, at, EndMode::BodyOrSemicolon).unwrap_or(line.byte_end());
        let symbol_id = ids.allocate(format!("{path}::{}:{name}", kind_label(kind)));
        let body = body_range(masked, at, end);
        let full_span = span_for_byte_range(source, at, end);
        let declaration = DartDeclaration {
            name: name.clone(),
            kind,
            span: anchor.clone(),
            extends: (kind == DartDeclarationKind::Class)
                .then(|| value_after_keyword(header, "extends"))
                .flatten(),
            mixes_in: if kind == DartDeclarationKind::Class {
                values_after_keyword(header, "with")
            } else {
                Vec::new()
            },
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

    let name = top_level_function(header.trim(), indent)?;
    let end = declaration_end(masked, at, EndMode::BodyOrSemicolon).unwrap_or(line.byte_end());
    let symbol_id = ids.allocate(format!("{path}::function:{name}"));
    let declaration = DartDeclaration {
        name,
        kind: DartDeclarationKind::Function,
        span: anchor,
        extends: None,
        mixes_in: Vec::new(),
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
    line_depths: &[usize],
    owner: &DeclarationRecord,
    records: &mut Vec<DeclarationRecord>,
    diagnostics: &mut Vec<DartDiagnostic>,
) {
    let Some((body_start, body_end)) = owner.body else {
        return;
    };
    let owner_id = owner.declaration.symbol_id.as_deref().unwrap_or_default();
    let owner_depth = brace_depth_at(masked, body_start) + 1;
    let member_start = if owner.declaration.kind == DartDeclarationKind::Enum {
        enum_member_start(masked, body_start, body_end, owner_depth).unwrap_or(body_end)
    } else {
        body_start + 1
    };
    let mut cursor = member_start;
    let mut ids = SymbolIdAllocator::default();

    for (index, line) in lines.iter().copied().enumerate() {
        if cursor >= body_end {
            break;
        }
        if line.byte_end() <= cursor {
            continue;
        }
        let mut at = first_code_byte(line, masked).max(cursor);
        while at < body_end && at < line.byte_end() {
            if depth_within_line(masked, line, line_depths[index], at) != owner_depth {
                break;
            }
            if masked.as_bytes()[at] == b'}' {
                break;
            }
            if skip_surplus_closer(masked, line, &mut at) {
                continue;
            }
            let declared_at = annotations_end(masked, at, body_end);
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
                    Some(SourceSpan::line(
                        line.number,
                        line.byte_start,
                        source_line_text(source, line),
                    )),
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
                    span: SourceSpan::line(
                        line.number,
                        line.byte_start,
                        source_line_text(source, line),
                    ),
                    extends: None,
                    mixes_in: Vec::new(),
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

    for line in lines.iter().copied() {
        if cursor >= body_end {
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
            let declared_at = annotations_end(masked, at, body_end);
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
                        span: SourceSpan::line(
                            line.number,
                            line.byte_start,
                            source_line_text(source, line),
                        ),
                        extends: None,
                        mixes_in: Vec::new(),
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
