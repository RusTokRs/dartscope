from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "crates/dartscope-index/tests/navigation_methods.rs",
    "let references = context.find_references(&[build_target.clone()]);",
    "let references = context.find_references(std::slice::from_ref(&build_target));",
)

replace_once(
    "crates/dartscope-parse/tests/identifier_references.rs",
    '''    assert_eq!(analysis.references.len(), 3);\n    assert_eq!(analysis.references[0].name, "load");\n    assert_eq!(analysis.references[0].prefix.as_deref(), Some("api"));\n    assert_eq!(analysis.references[0].confidence, Confidence::High);\n    assert_eq!(analysis.references[1].name, "Factory");\n    assert_eq!(analysis.references[1].prefix, None);\n    assert_eq!(analysis.references[1].confidence, Confidence::Medium);\n    assert_eq!(analysis.references[2].name, "client");\n''',
    '''    assert_eq!(analysis.references.len(), 4);\n    assert_eq!(analysis.references[0].name, "load");\n    assert_eq!(analysis.references[0].prefix.as_deref(), Some("api"));\n    assert_eq!(analysis.references[0].confidence, Confidence::High);\n    assert_eq!(analysis.references[1].name, "Factory");\n    assert_eq!(analysis.references[1].prefix, None);\n    assert_eq!(analysis.references[1].confidence, Confidence::Medium);\n    assert_eq!(analysis.references[2].name, "create");\n    assert_eq!(analysis.references[2].prefix.as_deref(), Some("Factory"));\n    assert_eq!(\n        analysis.references[2].kind,\n        DartIdentifierReferenceKind::MemberInvocationStatic\n    );\n    assert_eq!(analysis.references[2].confidence, Confidence::Medium);\n    assert_eq!(analysis.references[3].name, "client");\n''',
)
