from pathlib import Path

path = Path("crates/dartscope-index/src/navigation.rs")
text = path.read_text(encoding="utf-8")
old = """    let external_uris = external_namespace_uris(analysis, uri_graph, &reference);
    let base_status = definition_status(resolution.status, !external_uris.is_empty());
"""
new = """    let external_uris = external_namespace_uris(analysis, uri_graph, &reference);
    let base_status = if resolution.status
        == DartSymbolResolutionStatus::ConditionalEnvironmentRequired
        && resolution.candidates.is_empty()
        && !external_uris.is_empty()
    {
        DartDefinitionResolutionStatus::ExternalUnindexed
    } else {
        definition_status(resolution.status, !external_uris.is_empty())
    };
"""
if new in text:
    raise SystemExit(0)
if text.count(old) != 1:
    raise SystemExit(f"expected one constructor status match, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
