from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:80]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """        let mut references = analysis.references.clone();
        sort_references(&mut references);
""",
    """        let mut references = analysis.references.clone();
        suppress_redundant_constructor_invocations(&mut references);
        sort_references(&mut references);
""",
)

replace_once(
    "crates/dartscope-index/src/navigation.rs",
    """fn sort_references(references: &mut [DartIdentifierReference]) {
""",
    """fn suppress_redundant_constructor_invocations(
    references: &mut Vec<DartIdentifierReference>,
) {
    let constructor_facts = references
        .iter()
        .filter(|reference| reference.kind == DartIdentifierReferenceKind::ConstructorTarget)
        .map(reference_fact_key)
        .collect::<BTreeSet<_>>();
    references.retain(|reference| {
        reference.kind != DartIdentifierReferenceKind::InvocationTarget
            || !constructor_facts.contains(&reference_fact_key(reference))
    });
}

fn reference_fact_key(
    reference: &DartIdentifierReference,
) -> (String, usize, usize, String, Option<String>, Option<String>) {
    (
        reference.source_path.clone(),
        reference.span.byte_start,
        reference.span.byte_end,
        reference.name.clone(),
        reference.prefix.clone(),
        reference.enclosing_symbol_id.clone(),
    )
}

fn sort_references(references: &mut [DartIdentifierReference]) {
""",
)

replace_once(
    "crates/dartscope-index/tests/navigation_constructors.rs",
    """    let references = context.find_references(&[named_target.clone()]);
""",
    """    let references = context.find_references(std::slice::from_ref(&named_target));
""",
)
