use crate::pubspec_syntax::{flow_delimiters_are_balanced, prepare_pubspec_source};

#[test]
fn identifies_bare_wildcards_but_not_named_aliases() {
    let source = concat!("dependencies:\n", "  wildcard: *\n", "  alias: *defaults\n",);
    let prepared = prepare_pubspec_source(source);

    assert!(prepared.syntax.is_bare_wildcard_line(2));
    assert!(!prepared.syntax.is_bare_wildcard_line(3));
}

#[test]
fn retains_dependency_section_after_tab_indentation() {
    let source = concat!("dependencies:\n", "\tinvalid: any\n", "  wildcard: *\n",);
    let prepared = prepare_pubspec_source(source);

    assert!(prepared.syntax.is_bare_wildcard_line(3));
}

#[test]
fn accepts_single_document_markers() {
    let source = "---\r\nname: демо\r\n...\r\n";
    let prepared = prepare_pubspec_source(source);

    assert!(prepared.syntax.multiple_document_spans().is_empty());
    assert_eq!(prepared.source.len(), source.len());
    assert_eq!(prepared.source.lines().count(), source.lines().count());
    assert!(prepared.source.contains("name: демо"));
    assert!(!prepared.source.contains("---"));
    assert!(!prepared.source.contains("..."));
}

#[test]
fn masks_additional_documents_without_changing_offsets() {
    let source = "name: first\n---\nname: second\n";
    let prepared = prepare_pubspec_source(source);

    assert_eq!(prepared.source.len(), source.len());
    assert!(prepared.source.contains("name: first"));
    assert!(!prepared.source.contains("name: second"));
    assert_eq!(prepared.syntax.multiple_document_spans()[0].start_line, 2);
}

#[test]
fn detects_duplicate_top_level_and_direct_mapping_keys() {
    let source = concat!(
        "name: first\n",
        "name: second\n",
        "dependencies:\n",
        "  shared: ^1.0.0\n",
        "  shared: ^2.0.0\n",
        "flutter:\n",
        "  generate: true\n",
        "  generate: false\n",
    );
    let prepared = prepare_pubspec_source(source);
    let duplicates = prepared.syntax.duplicate_keys();

    assert_eq!(duplicates.len(), 3);
    assert_eq!(duplicates[0].key, "name");
    assert_eq!(duplicates[0].span.start_line, 2);
    assert_eq!(duplicates[1].key, "shared");
    assert_eq!(duplicates[2].key, "generate");
}

#[test]
fn rejects_unbalanced_flow_delimiters_and_quotes() {
    for value in [
        "{ path: ../local } }",
        "{ git: { url: https://example.com/repo.git ] }",
        "{ path: \"unterminated }",
    ] {
        assert!(!flow_delimiters_are_balanced(value), "{value}");
    }
}

#[test]
fn accepts_nested_flow_mappings_with_quoted_commas() {
    assert!(flow_delimiters_are_balanced(
        "{ git: { url: \"https://example.com/repo.git?parts=one,two\", ref: stable } }"
    ));
}

#[test]
fn accepts_yaml_single_quote_escaping() {
    assert!(flow_delimiters_are_balanced(
        "{ path: 'packages/it''s-local' }"
    ));
}
