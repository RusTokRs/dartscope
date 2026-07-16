//! Normalized declaration inventory for the conservative parser backend.

mod scanner;
mod syntax;

use dartscope_core::{DartDeclaration, DartDeclarationKind, DartDiagnostic, SourceSpan};

use self::scanner::{
    EndMode, body_range, brace_depth_at, declaration_end, declaration_header, enum_member_start,
    first_code_byte, line_brace_depths, source_line_text,
};
use self::syntax::{
    SymbolIdAllocator, has_primary_constructor, is_callable_kind, is_concise_constructor,
    is_directive, is_type_kind, kind_label, local_variable_names, member_headers, type_header,
};
use crate::declarations::{
    top_level_function, top_level_variable, value_after_keyword, values_after_keyword,
};
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
        collect_locals(path, source, masked_source, &lines, &callable, &mut records);
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
    let mut skip_until = 0usize;
    let mut ids = SymbolIdAllocator::default();

    for (index, line) in lines.iter().copied().enumerate() {
        if line.text.trim().is_empty() {
            continue;
        }
        let start = first_code_byte(line, masked);
        if start < skip_until || line_depths[index] != 0 {
            continue;
        }
        let Some(header) = declaration_header(masked, start) else {
            continue;
        };
        if header.trim_start().starts_with('@') || is_directive(header) {
            continue;
        }

        if let Some((name, kind)) = type_header(header) {
            let end =
                declaration_end(masked, start, EndMode::BodyOrSemicolon).unwrap_or(line.byte_end());
            let symbol_id = ids.allocate(format!("{path}::{}:{name}", kind_label(kind)));
            let body = body_range(masked, start, end);
            let anchor =
                SourceSpan::line(line.number, line.byte_start, source_line_text(source, line));
            let full_span = span_for_byte_range(source, start, end);
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
            skip_until = end;
            records.push(DeclarationRecord { declaration, body });
            continue;
        }

        let indent = line
            .text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();
        let variable = top_level_variable(header.trim(), indent)
            .map(|name| (name, DartDeclarationKind::Variable, EndMode::SemicolonOnly));
        let function = top_level_function(header.trim(), indent).map(|name| {
            (
                name,
                DartDeclarationKind::Function,
                EndMode::BodyOrSemicolon,
            )
        });
        if let Some((name, kind, mode)) = variable.or(function) {
            let end = declaration_end(masked, start, mode).unwrap_or(line.byte_end());
            let symbol_id = ids.allocate(format!("{path}::{}:{name}", kind_label(kind)));
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
                parent_symbol_id: None,
                declaration_span: Some(span_for_byte_range(source, start, end)),
            };
            skip_until = end;
            records.push(DeclarationRecord {
                declaration,
                body: body_range(masked, start, end),
            });
        }
    }

    records
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
    let mut skip_until = member_start;
    let mut ids = SymbolIdAllocator::default();

    for (index, line) in lines.iter().copied().enumerate() {
        if line.text.trim().is_empty() {
            continue;
        }
        let start = first_code_byte(line, masked);
        if start < member_start
            || start >= body_end
            || start < skip_until
            || line_depths[index] != owner_depth
        {
            continue;
        }
        let Some(header) = declaration_header(masked, start) else {
            continue;
        };
        let header = header.trim();
        if header.is_empty() || header.starts_with('@') || header.starts_with("case ") {
            continue;
        }
        if is_concise_constructor(header) {
            diagnostics.push(DartDiagnostic::warning(
                "unsupported_concise_constructor",
                "concise constructor syntax requires Dart 3.13 language-version handling",
                Some(SourceSpan::line(
                    line.number,
                    line.byte_start,
                    source_line_text(source, line),
                )),
            ));
            skip_until =
                declaration_end(masked, start, EndMode::BodyOrSemicolon).unwrap_or(line.byte_end());
            continue;
        }

        let members = member_headers(header, &owner.declaration.name);
        let Some((_, _, mode)) = members.first() else {
            continue;
        };
        let end = declaration_end(masked, start, *mode).unwrap_or(line.byte_end());
        let full_span = span_for_byte_range(source, start, end);
        let body = body_range(masked, start, end);
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
        skip_until = end;
    }
}

fn collect_locals(
    _path: &str,
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
    let mut skip_until = body_start + 1;
    let mut ids = SymbolIdAllocator::default();

    for line in lines.iter().copied() {
        if line.text.trim().is_empty() {
            continue;
        }
        let start = first_code_byte(line, masked);
        if start <= body_start || start >= body_end || start < skip_until {
            continue;
        }
        let Some(header) = declaration_header(masked, start) else {
            continue;
        };
        let names = local_variable_names(header.trim());
        if names.is_empty() {
            continue;
        }
        let end = declaration_end(masked, start, EndMode::SemicolonOnly).unwrap_or(line.byte_end());
        let full_span = span_for_byte_range(source, start, end);
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
        skip_until = end;
    }
}
