import base64
import io
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


core_path = ROOT / "crates/dartscope-core/src/lib.rs"
core = core_path.read_text(encoding="utf-8")
if "MemberOperatorDeclaration" in core:
    print("member audit and operator patch already applied")
    raise SystemExit(0)

archive_text = (ROOT / "tools/agent/member-audit-operators.tar.gz.b64").read_text(encoding="utf-8")
archive = base64.b64decode(archive_text)
with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as bundle:
    bundle.extractall(ROOT, filter="data")

core = replace_once(
    core,
    "    MemberPropertyWriteInstance,\n    MemberPropertyWriteStatic,\n",
    "    MemberPropertyWriteInstance,\n    MemberPropertyWriteStatic,\n    MemberOperatorDeclaration,\n    MemberOperatorInvocationInstance,\n",
    "core operator reference variants",
)
core_path.write_text(core, encoding="utf-8")

parse_lib_path = ROOT / "crates/dartscope-parse/src/lib.rs"
parse_lib = parse_lib_path.read_text(encoding="utf-8")
parse_lib = replace_once(
    parse_lib,
    "mod member_references;\nmod namespace;\nmod property_references;\n",
    "mod member_reference_syntax;\nmod member_references;\nmod namespace;\nmod operator_references;\nmod property_references;\n",
    "parse member modules",
)
parse_lib_path.write_text(parse_lib, encoding="utf-8")

analysis_path = ROOT / "crates/dartscope-parse/src/analysis.rs"
analysis = analysis_path.read_text(encoding="utf-8")
analysis = replace_once(
    analysis,
    "use crate::namespace::{directive_uri, extract_namespace_directives};\nuse crate::property_references::collect_property_references;\n",
    "use crate::namespace::{directive_uri, extract_namespace_directives};\nuse crate::operator_references::collect_operator_references;\nuse crate::property_references::collect_property_references;\n",
    "analysis operator import",
)
property_call = """    references.extend(collect_property_references(
        &source,
        &lexical.code,
        &file,
        &bindings,
    ));
"""
analysis = replace_once(
    analysis,
    property_call,
    property_call
    + """    references.extend(collect_operator_references(
        &source,
        &lexical.code,
        &file,
    ));
""",
    "file operator collection",
)
project_property_call = """        file_references.extend(collect_property_references(
            source,
            &lexical.code,
            file,
            &file_bindings,
        ));
"""
analysis = replace_once(
    analysis,
    project_property_call,
    project_property_call
    + """        file_references.extend(collect_operator_references(
            source,
            &lexical.code,
            file,
        ));
""",
    "project operator collection",
)
analysis_path.write_text(analysis, encoding="utf-8")

method_path = ROOT / "crates/dartscope-parse/src/member_references.rs"
method = method_path.read_text(encoding="utf-8")
method = replace_once(
    method,
    "    DartIdentifierReferenceKind, DartInvocation, DartLexicalBinding, SourceSpan,\n",
    "    DartIdentifierReferenceKind, DartInvocation, DartLexicalBinding,\n",
    "method imports",
)
method = replace_once(
    method,
    "use crate::source_lines::span_for_byte_range;\n",
    "use crate::member_reference_syntax::{\n    declaration_is_static, declaration_name_range, looks_like_type_name,\n};\nuse crate::source_lines::span_for_byte_range;\n",
    "method syntax import",
)
start = method.index("fn declaration_name_range(")
end = method.index("fn invocation_member_range(")
method = method[:start] + method[end:]
looks = """fn looks_like_type_name(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
}

"""
method = replace_once(method, looks, "", "method local type-name helper")
method_path.write_text(method, encoding="utf-8")

property_path = ROOT / "crates/dartscope-parse/src/property_references.rs"
property_text = property_path.read_text(encoding="utf-8")
property_text = replace_once(
    property_text,
    "    DartIdentifierReferenceKind, DartLexicalBinding, SourceSpan,\n",
    "    DartIdentifierReferenceKind, DartLexicalBinding,\n",
    "property imports",
)
property_text = replace_once(
    property_text,
    "use crate::source_lines::span_for_byte_range;\n",
    "use crate::member_reference_syntax::{\n    declaration_is_static, declaration_name_range, declaration_span, looks_like_type_name,\n};\nuse crate::source_lines::span_for_byte_range;\n",
    "property syntax import",
)
start = property_text.index("fn declaration_name_range(")
end = property_text.index("fn binding_is_visible(")
property_text = property_text[:start] + property_text[end:]
looks = """fn looks_like_type_name(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
}

"""
property_text = replace_once(property_text, looks, "", "property local type-name helper")
property_path.write_text(property_text, encoding="utf-8")

namespace_path = ROOT / "crates/dartscope-index/src/namespace.rs"
namespace = namespace_path.read_text(encoding="utf-8")
constructible = """pub(crate) fn resolve_constructible_type_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    resolve_symbol_with_resolver_filter(project, query, resolver, |kind| {
        matches!(
            kind,
            DartDeclarationKind::Class | DartDeclarationKind::ExtensionType
        )
    })
}
"""
namespace = replace_once(
    namespace,
    constructible,
    constructible
    + """

pub(crate) fn resolve_member_owner_with_resolver(
    project: &DartProjectAnalysis,
    query: DartSymbolQuery,
    resolver: &NamespaceResolver<'_, '_>,
) -> DartSymbolResolution {
    resolve_symbol_with_resolver_filter(project, query, resolver, |kind| {
        matches!(
            kind,
            DartDeclarationKind::Class
                | DartDeclarationKind::Mixin
                | DartDeclarationKind::Enum
                | DartDeclarationKind::Extension
                | DartDeclarationKind::ExtensionType
        )
    })
}
""",
    "member owner resolver",
)
namespace_path.write_text(namespace, encoding="utf-8")

navigation_path = ROOT / "crates/dartscope-index/src/navigation.rs"
navigation = navigation_path.read_text(encoding="utf-8")
navigation = "mod members;\n\n" + navigation
index_start = navigation.index("#[derive(Debug, Clone, Eq, PartialEq)]\nstruct IndexedMethod")
index_end = navigation.index("/// Reusable resolution context")
navigation = navigation[:index_start] + navigation[index_end:]
navigation = replace_once(
    navigation,
    "        let member_index = MemberIndex::new(analysis);",
    "        let member_index = members::MemberIndex::new(analysis);",
    "member index construction",
)
navigation = replace_once(
    navigation,
    "                            && !is_member_declaration_kind(resolution.reference.kind)",
    "                            && !members::is_declaration_kind(resolution.reference.kind)",
    "reverse member declaration filter",
)
navigation = replace_once(
    navigation,
    "    member_index: &MemberIndex,",
    "    member_index: &members::MemberIndex,",
    "resolve member index type",
)
dispatch_start = navigation.index("    if is_method_declaration_kind(reference.kind) {")
dispatch_end = navigation.index(
    "    if matches!(\n        reference.kind,\n        DartIdentifierReferenceKind::VariableRead",
    dispatch_start,
)
navigation = (
    navigation[:dispatch_start]
    + """    if let Some(resolution) = members::resolve_reference(
        analysis,
        namespace,
        uri_graph,
        member_index,
        reference.clone(),
    ) {
        return resolution;
    }
"""
    + navigation[dispatch_end:]
)
member_start = navigation.index("fn resolve_method_declaration_reference(")
constructor_start = navigation.index("fn resolve_constructor_reference(")
navigation = navigation[:member_start] + navigation[constructor_start:]
navigation_path.write_text(navigation, encoding="utf-8")

references_path = ROOT / "crates/dartscope-index/src/references.rs"
references = references_path.read_text(encoding="utf-8")
references = replace_once(
    references,
    "                    | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n",
    "                    | DartIdentifierReferenceKind::MemberPropertyWriteStatic\n                    | DartIdentifierReferenceKind::MemberOperatorDeclaration\n                    | DartIdentifierReferenceKind::MemberOperatorInvocationInstance\n",
    "legacy operator reference filter",
)
references_path.write_text(references, encoding="utf-8")

print("applied member audit corrections and direct operator targets")
