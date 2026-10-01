use std::collections::HashMap;

use dartscope_core::DartDeclarationKind;

use super::scanner::EndMode;
use crate::declarations::{
    callable_name, class_declaration_name, extension_declaration_name,
    extension_type_declaration_name, extension_type_parameters, mixin_application_superclass,
    mixin_declaration_name, name_after_keyword, value_after_keyword, values_after_keyword,
};
use crate::identifiers::{is_identifier, is_identifier_continue, leading_identifier};

pub(super) fn type_header(header: &str) -> Option<(String, DartDeclarationKind)> {
    class_declaration_name(header)
        .map(|name| (name, DartDeclarationKind::Class))
        .or_else(|| mixin_declaration_name(header).map(|name| (name, DartDeclarationKind::Mixin)))
        .or_else(|| {
            name_after_keyword(header, "enum").map(|name| (name, DartDeclarationKind::Enum))
        })
        .or_else(|| {
            extension_type_declaration_name(header)
                .map(|name| (name, DartDeclarationKind::ExtensionType))
        })
        .or_else(|| {
            extension_declaration_name(header).map(|name| (name, DartDeclarationKind::Extension))
        })
        .or_else(|| {
            name_after_keyword(header, "typedef").map(|name| (name, DartDeclarationKind::Typedef))
        })
}

pub(super) fn member_headers(
    header: &str,
    owner_name: &str,
) -> Vec<(String, DartDeclarationKind, EndMode)> {
    let cleaned = strip_member_modifiers(header);
    let before_paren = cleaned.split_once('(').map(|(left, _)| left.trim());

    if let Some(before) = before_paren
        && (before == owner_name
            || before
                .strip_prefix(owner_name)
                .is_some_and(|rest| rest.strip_prefix('.').is_some_and(is_identifier)))
    {
        return vec![(
            before.to_string(),
            DartDeclarationKind::Constructor,
            EndMode::BodyOrSemicolon,
        )];
    }

    if let Some(name) = name_after_token(cleaned, "get") {
        return vec![(
            name,
            DartDeclarationKind::Getter,
            callable_end_mode(cleaned),
        )];
    }
    if let Some(name) = name_after_token(cleaned, "set") {
        return vec![(
            name,
            DartDeclarationKind::Setter,
            callable_end_mode(cleaned),
        )];
    }
    if let Some(name) = operator_name(cleaned) {
        return vec![(
            name,
            DartDeclarationKind::Operator,
            EndMode::BodyOrSemicolon,
        )];
    }

    if let Some(before) = before_paren {
        if before.contains('=') || starts_control_keyword(before) {
            return Vec::new();
        }
        if let Some(name) = callable_name(cleaned) {
            return vec![(
                name,
                DartDeclarationKind::Method,
                callable_end_mode(cleaned),
            )];
        }
    }

    field_names(cleaned)
        .into_iter()
        .map(|name| (name, DartDeclarationKind::Field, EndMode::SemicolonOnly))
        .collect()
}

/// How far a callable declaration extends from its header.
///
/// A body is the first `{ ... }` block, but an arrow body (`=> expression;`) ends at the first
/// semicolon outside brackets even when the expression itself starts with a brace, for example a map
/// or set literal.
pub(super) fn callable_end_mode(header: &str) -> EndMode {
    if header.trim_end().ends_with("=>") {
        EndMode::SemicolonOnly
    } else {
        EndMode::BodyOrSemicolon
    }
}

/// Name and kind of a top-level getter or setter, such as `int get total => 1;` or
/// `set total(int value) {}`.
///
/// Variables and functions are recognized before accessors, so a variable that happens to be called
/// `get` or `set` is never mistaken for one.
pub(super) fn top_level_accessor(
    header: &str,
    indent: usize,
) -> Option<(String, DartDeclarationKind)> {
    if indent != 0 {
        return None;
    }
    let cleaned = strip_member_modifiers(header);
    if let Some(name) = name_after_token(cleaned, "get") {
        return Some((name, DartDeclarationKind::Getter));
    }
    name_after_token(cleaned, "set").map(|name| (name, DartDeclarationKind::Setter))
}

/// The `extends`, `with`, and `on` clauses of a type header, kept apart so an `on` constraint is never
/// mistaken for a base class or a mixed-in type.
#[derive(Default)]
pub(super) struct TypeRelations {
    pub(super) extends: Option<String>,
    pub(super) mixes_in: Vec<String>,
    pub(super) on_types: Vec<String>,
}

pub(super) fn type_relations(header: &str, kind: DartDeclarationKind) -> TypeRelations {
    match kind {
        DartDeclarationKind::Class => TypeRelations {
            extends: value_after_keyword(header, "extends")
                .or_else(|| mixin_application_superclass(header)),
            mixes_in: values_after_keyword(header, "with"),
            on_types: Vec::new(),
        },
        DartDeclarationKind::Enum => TypeRelations {
            mixes_in: values_after_keyword(header, "with"),
            ..TypeRelations::default()
        },
        DartDeclarationKind::Mixin => TypeRelations {
            on_types: values_after_keyword(header, "on"),
            ..TypeRelations::default()
        },
        DartDeclarationKind::Extension => {
            let type_parameters = extension_type_parameters(header);
            TypeRelations {
                // An `on` type that is one of the extension's own type parameters applies to every
                // receiver, which an empty list expresses.
                on_types: value_after_keyword(header, "on")
                    .filter(|on_type| !type_parameters.contains(on_type))
                    .into_iter()
                    .collect(),
                ..TypeRelations::default()
            }
        }
        _ => TypeRelations::default(),
    }
}

/// One enum constant: its name and the byte range of its declaration, which starts at the name
/// (annotations excluded, like every other declaration) and ends before the separator.
pub(super) struct EnumConstant {
    pub(super) name: String,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Collects the constants of an enum body: the comma-separated entries before the first
/// top-level semicolon (or before the closing brace when there is none).
///
/// `masked` has comments and strings blanked, so separators inside constructor arguments, type
/// arguments, or nested blocks are skipped by tracking bracket depth.
pub(super) fn enum_constants(
    masked: &str,
    body_start: usize,
    body_end: usize,
) -> Vec<EnumConstant> {
    let end = body_end.min(masked.len());
    let body = masked
        .as_bytes()
        .get(body_start + 1..end)
        .unwrap_or_default();
    let mut constants = Vec::new();
    let mut nesting = 0usize;
    let mut angles = 0usize;
    let mut segment_start = body_start + 1;
    // The closing brace ends the last constant exactly like a semicolon does, so a terminating `;`
    // stands in for it.
    for (offset, &byte) in body.iter().chain(std::iter::once(&b';')).enumerate() {
        let index = body_start + 1 + offset;
        match byte {
            b'(' | b'[' | b'{' => nesting += 1,
            b')' | b']' | b'}' => nesting = nesting.saturating_sub(1),
            b'<' if nesting == 0 => angles += 1,
            b'>' if nesting == 0 => angles = angles.saturating_sub(1),
            b',' | b';' if nesting == 0 && angles == 0 => {
                push_enum_constant(masked, segment_start, index.min(end), &mut constants);
                segment_start = index + 1;
                if byte == b';' {
                    break;
                }
            }
            _ => {}
        }
    }
    constants
}

fn push_enum_constant(masked: &str, start: usize, end: usize, constants: &mut Vec<EnumConstant>) {
    let Some(segment) = masked.get(start..end) else {
        return;
    };
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return;
    }
    let constant_start = start + (segment.len() - segment.trim_start().len());
    let constant_end = constant_start + trimmed.len();
    let declared_at = crate::metadata::annotations_end(masked, constant_start, constant_end);
    let after_annotations = &masked[declared_at..constant_end];
    let name_offset = after_annotations.len() - after_annotations.trim_start().len();
    let Some(name) = leading_identifier(&after_annotations[name_offset..]) else {
        return;
    };
    constants.push(EnumConstant {
        name: name.to_string(),
        start: declared_at + name_offset,
        end: constant_end,
    });
}

pub(super) fn local_variable_names(header: &str) -> Vec<String> {
    let header = header.strip_prefix("late ").unwrap_or(header);
    if starts_control_keyword(header)
        || ["return ", "throw ", "yield ", "await ", "case "]
            .iter()
            .any(|prefix| header.starts_with(prefix))
    {
        return Vec::new();
    }
    if let Some(without_keyword) = ["var", "final", "const"]
        .into_iter()
        .find_map(|keyword| header.strip_prefix(keyword).map(str::trim_start))
    {
        declared_names(without_keyword, false)
    } else {
        declared_names(header, true)
    }
}

fn field_names(header: &str) -> Vec<String> {
    if header.starts_with("return ") || header.starts_with("throw ") || header.contains("=>") {
        return Vec::new();
    }
    declared_names(header, true)
}

/// Normalized names of a top-level `const`/`final`/`var` or explicitly typed variable declaration.
///
/// The keyword forms and the explicitly typed form share the field/declarator splitting rules, so
/// `int counter = 0;`, `late final int total = 1;`, and `int first, second;` are all recognized while
/// callable headers, directives, and control statements are rejected by the same guards.
pub(super) fn top_level_variables(header: &str, indent: usize) -> Vec<String> {
    if indent != 0 {
        return Vec::new();
    }
    let header = header
        .strip_prefix("late ")
        .map(str::trim_start)
        .unwrap_or(header);
    if starts_control_keyword(header) || header.starts_with("get ") || header.starts_with("set ") {
        return Vec::new();
    }
    // A keyword declaration keeps its name even when the initializer is a multi-line expression whose
    // first top-level arrow token ends the scanned header, because `final x = ...` is never a getter.
    if let Some(without_keyword) = ["var", "final", "const"]
        .into_iter()
        .find_map(|keyword| header.strip_prefix(keyword).map(str::trim_start))
    {
        let without_keyword = without_keyword
            .strip_prefix("late ")
            .map(str::trim_start)
            .unwrap_or(without_keyword);
        return declared_names(without_keyword, false);
    }
    if is_accessor_header(header) {
        return Vec::new();
    }
    field_names(header)
}

/// Whether the text before any assignment is a getter or setter header such as `external int get
/// total;` or `set total(int value)`: a `get` or `set` token that is followed by the accessor's name.
/// A variable that is merely called `get` (`int get = 0;`) has no token after it.
fn is_accessor_header(header: &str) -> bool {
    let tokens: Vec<&str> = assignment_left(header).split_whitespace().collect();
    tokens
        .iter()
        .enumerate()
        .any(|(index, token)| matches!(*token, "get" | "set") && index + 1 < tokens.len())
}

fn declared_names(header: &str, require_type: bool) -> Vec<String> {
    let header = header.trim_end_matches(';').trim();
    if header.is_empty() {
        return Vec::new();
    }

    let segments = split_top_level_commas(header);
    let mut names = Vec::new();
    for (index, segment) in segments.into_iter().enumerate() {
        let declarator = declarator_name(assignment_left(segment).trim(), index == 0 && require_type);
        match declarator {
            Some(name) => names.push(name.to_string()),
            // The first declarator carries the keyword or the type. When it is neither, the text is
            // a call, a labelled argument or a similar statement, and the segments after its commas
            // are arguments rather than further declarators.
            None if index == 0 => return Vec::new(),
            None => {}
        }
    }
    names
}

/// The variable declared by one comma-separated declarator, or `None` when the text is not one.
///
/// `typed` demands a type before the name, which is how a statement such as `a = b` or `call()`
/// is told apart from `int a`.
fn declarator_name(left: &str, typed: bool) -> Option<&str> {
    // A colon belongs to a labelled statement, a named argument, a map entry or a conditional
    // expression; no declarator has one before its initializer.
    if left.contains(':') {
        return None;
    }
    let candidate = if left.contains('(') {
        // Parentheses are only a declarator when they belong to the variable's type: a function
        // type (`void Function(int) onTap`) or a record type (`(int, int) point`).
        function_typed_declarator(left)?
    } else {
        left.split_whitespace().last()?
    };
    let candidate = candidate.trim_start_matches(['?', '!']);
    if !is_identifier(candidate) {
        return None;
    }
    if typed && left.split_whitespace().count() < 2 {
        return None;
    }
    Some(candidate)
}

/// The declared name of a variable whose type is a function type or a record type, such as
/// `void Function(int)? onTap` or `(int, int) point`.
///
/// Any other text with parentheses is a call or a statement rather than a declaration, so it yields
/// no name.
fn function_typed_declarator(left: &str) -> Option<&str> {
    if !(left.starts_with('(') || contains_function_type(left)) {
        return None;
    }
    let mut depth = 0usize;
    let mut last_close = None;
    for (index, byte) in left.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    last_close = Some(index);
                }
            }
            _ => {}
        }
    }
    let after = left[last_close? + 1..].trim_start_matches('?').trim();
    is_identifier(after).then_some(after)
}

/// Whether `text` names a function type: the word `Function` followed by a parameter list, with or
/// without type arguments in between.
fn contains_function_type(text: &str) -> bool {
    text.match_indices("Function").any(|(index, word)| {
        let standalone = text[..index]
            .bytes()
            .next_back()
            .is_none_or(|byte| !is_identifier_continue(byte));
        let next = text[index + word.len()..].trim_start().bytes().next();
        standalone && matches!(next, Some(b'(' | b'<'))
    })
}

fn split_top_level_commas(value: &str) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b',' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                parts.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

fn strip_member_modifiers(mut header: &str) -> &str {
    loop {
        let trimmed = header.trim_start();
        let Some((first, rest)) = trimmed.split_once(char::is_whitespace) else {
            return trimmed;
        };
        if is_member_modifier(first) {
            header = rest;
        } else {
            return trimmed;
        }
    }
}

fn is_member_modifier(token: &str) -> bool {
    matches!(
        token,
        "abstract"
            | "augment"
            | "const"
            | "covariant"
            | "external"
            | "factory"
            | "final"
            | "late"
            | "static"
    )
}

fn name_after_token(header: &str, token: &str) -> Option<String> {
    let tokens: Vec<_> = header.split_whitespace().collect();
    let index = tokens.iter().position(|item| *item == token)?;
    tokens.get(index + 1).and_then(|item| {
        let name = leading_identifier(item)?;
        name.bytes()
            .all(is_identifier_continue)
            .then(|| name.to_string())
    })
}

fn operator_name(header: &str) -> Option<String> {
    let (_, rest) = header.split_once("operator")?;
    let token = rest.split_whitespace().next()?;
    let name: String = token.chars().take_while(|ch| *ch != '(').collect();
    let name = name.as_str();
    matches!(
        name,
        "<" | ">"
            | "<="
            | ">="
            | "=="
            | "~"
            | "-"
            | "+"
            | "/"
            | "~/"
            | "*"
            | "%"
            | "|"
            | "^"
            | "&"
            | "<<"
            | ">>>"
            | ">>"
            | "[]="
            | "[]"
    )
    .then(|| name.to_string())
}

/// Reports a constructor written in the Dart 3.13 concise-constructor form.
///
/// A leading `new` is always the concise form because it is not a constructor-declaration keyword in
/// earlier Dart syntax. A leading `factory` is the concise form only when it is not qualified by the
/// declaring type, so `factory A.named()` and `factory A()` remain ordinary declarations that
/// `member_headers` collects instead of being skipped with a fabricated diagnostic.
pub(super) fn is_concise_constructor(header: &str, owner_name: &str) -> bool {
    let mut rest = header.trim_start();
    loop {
        let Some((token, tail)) = split_first_token(rest) else {
            return false;
        };
        match token {
            "new" => return true,
            "factory" => {
                let qualified = tail
                    .trim_start()
                    .strip_prefix(owner_name)
                    .is_some_and(|rest| rest.starts_with('(') || rest.starts_with('.'));
                return !qualified;
            }
            _ if is_member_modifier(token) => rest = tail,
            _ => return false,
        }
    }
}

fn split_first_token(value: &str) -> Option<(&str, &str)> {
    let trimmed = value.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed
        .find(|ch: char| ch.is_whitespace() || ch == '(')
        .unwrap_or(trimmed.len());
    Some((&trimmed[..end], &trimmed[end..]))
}

pub(super) fn has_primary_constructor(header: &str, name: &str) -> bool {
    let Some(index) = header.find(name) else {
        return false;
    };
    header[index + name.len()..].trim_start().starts_with('(')
}

pub(super) fn is_directive(header: &str) -> bool {
    ["import ", "export ", "part ", "part of ", "library "]
        .iter()
        .any(|prefix| header.trim_start().starts_with(prefix))
}

fn starts_control_keyword(value: &str) -> bool {
    [
        "if", "for", "while", "switch", "catch", "return", "throw", "assert",
    ]
    .iter()
    .any(|keyword| value == *keyword || value.starts_with(&format!("{keyword} ")))
}

pub(super) fn is_type_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Class
            | DartDeclarationKind::Mixin
            | DartDeclarationKind::Enum
            | DartDeclarationKind::Extension
            | DartDeclarationKind::ExtensionType
    )
}

pub(super) fn is_callable_kind(kind: DartDeclarationKind) -> bool {
    matches!(
        kind,
        DartDeclarationKind::Function
            | DartDeclarationKind::Method
            | DartDeclarationKind::Constructor
            | DartDeclarationKind::Getter
            | DartDeclarationKind::Setter
            | DartDeclarationKind::Operator
    )
}

pub(super) fn kind_label(kind: DartDeclarationKind) -> &'static str {
    match kind {
        DartDeclarationKind::Class => "class",
        DartDeclarationKind::Mixin => "mixin",
        DartDeclarationKind::Enum => "enum",
        DartDeclarationKind::Extension => "extension",
        DartDeclarationKind::ExtensionType => "extension_type",
        DartDeclarationKind::Typedef => "typedef",
        DartDeclarationKind::Function => "function",
        DartDeclarationKind::Variable => "variable",
        DartDeclarationKind::Method => "method",
        DartDeclarationKind::Constructor => "constructor",
        DartDeclarationKind::Field => "field",
        DartDeclarationKind::Getter => "getter",
        DartDeclarationKind::Setter => "setter",
        DartDeclarationKind::Operator => "operator",
        DartDeclarationKind::LocalVariable => "local_variable",
    }
}

#[derive(Default)]
pub(super) struct SymbolIdAllocator {
    counts: HashMap<String, usize>,
}

impl SymbolIdAllocator {
    pub(super) fn allocate(&mut self, base: String) -> String {
        let count = self.counts.entry(base.clone()).or_default();
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}#{}", *count)
        }
    }
}

/// Returns the text before the first top-level assignment `=`.
///
/// Arrow (`=>`), equality (`==`), and comparison (`<=`, `>=`, `!=`) operators are not assignments, and
/// an `=` nested inside parentheses, brackets, braces, or type arguments belongs to a nested
/// expression such as a default value. Splitting on the real assignment keeps a function-typed
/// declaration from reporting its first parameter as the declared name.
fn assignment_left(segment: &str) -> &str {
    let bytes = segment.as_bytes();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut angles = 0usize;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if parens == 0 && brackets == 0 && braces == 0 => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            b'=' if parens == 0 && brackets == 0 && braces == 0 && angles == 0 => {
                let next = bytes.get(index + 1).copied();
                let previous = index.checked_sub(1).map(|position| bytes[position]);
                if next != Some(b'>')
                    && !matches!(previous, Some(b'=') | Some(b'!') | Some(b'<') | Some(b'>'))
                {
                    return &segment[..index];
                }
            }
            _ => {}
        }
    }
    segment
}
