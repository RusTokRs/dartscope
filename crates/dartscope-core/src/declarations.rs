//! Declarations, string constants, invocations and their arguments.

use serde::{Deserialize, Serialize};

use crate::span::SourceSpan;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartDeclaration {
    pub name: String,
    pub kind: DartDeclarationKind,
    pub span: SourceSpan,
    pub extends: Option<String>,
    pub mixes_in: Vec<String>,
    /// Types named by an `on` clause: the superclass constraints of a `mixin` or the extended type
    /// of an `extension`. Empty for every other declaration, and for an extension whose `on` type is
    /// one of its own type parameters (`extension X<T> on T` applies to any receiver).
    ///
    /// `extends` and `mixes_in` describe class headers only (`extends` / `with`); an `on` clause is
    /// reported here instead so it is never mistaken for a base class or a mixed-in type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_symbol_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartStringConstant {
    pub name: String,
    pub value: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DartDeclarationKind {
    Class,
    Mixin,
    Enum,
    Extension,
    ExtensionType,
    Typedef,
    Function,
    Variable,
    Method,
    Constructor,
    Field,
    Getter,
    Setter,
    Operator,
    LocalVariable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartInvocation {
    /// Normalized dotted target such as `Image.asset` or `DefaultAssetBundle.of.loadString`.
    pub target: String,
    pub arguments: Vec<DartInvocationArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_members: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_symbol_id: Option<String>,
    /// Exact invocation expression span.
    pub span: SourceSpan,
    /// Complete source-line evidence retained for compatibility projections.
    pub source_line_span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartInvocationArgument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Original source expression with surrounding whitespace removed.
    pub expression: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_entries: Vec<DartMapEntry>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartMapEntry {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_key: Option<String>,
    pub value: String,
    pub span: SourceSpan,
    pub source_line_span: SourceSpan,
}
