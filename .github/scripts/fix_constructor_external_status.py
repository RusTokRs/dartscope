from pathlib import Path

path = Path("crates/dartscope-index/src/navigation.rs")
text = path.read_text(encoding="utf-8")
old = """        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => {
            DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
        }
"""
new = """        DartSymbolResolutionStatus::ConditionalEnvironmentRequired if has_external_uris => {
            DartDefinitionResolutionStatus::ExternalUnindexed
        }
        DartSymbolResolutionStatus::ConditionalEnvironmentRequired => {
            DartDefinitionResolutionStatus::ConditionalEnvironmentRequired
        }
"""
if new in text:
    raise SystemExit(0)
if text.count(old) != 1:
    raise SystemExit(f"expected one status match, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
