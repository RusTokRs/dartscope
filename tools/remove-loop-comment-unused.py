from pathlib import Path

path = Path("crates/dartscope-parse/src/lexical_regions/controls.rs")
source = path.read_text(encoding="utf-8")
for signature in [
    "fn if_statement_end(source: &str, keyword_end: usize) -> Option<usize> {",
    "fn try_statement_end(source: &str, keyword_end: usize) -> Option<usize> {",
    "fn is_await_for(source: &str, token: super::IdentifierToken<'_>) -> bool {",
]:
    old = f"{signature}\n    let bytes = source.as_bytes();\n"
    new = f"{signature}\n"
    if source.count(old) != 1:
        raise SystemExit(f"unused bytes declaration not found for {signature}")
    source = source.replace(old, new, 1)
path.write_text(source, encoding="utf-8")
