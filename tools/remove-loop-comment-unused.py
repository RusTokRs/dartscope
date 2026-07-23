from pathlib import Path

path = Path("crates/dartscope-parse/src/lexical_regions/controls.rs")
source = path.read_text(encoding="utf-8")
declaration = "    let bytes = source.as_bytes();\n"
for signature in [
    "fn if_statement_end(source: &str, keyword_end: usize) -> Option<usize> {",
    "fn try_statement_end(source: &str, keyword_end: usize) -> Option<usize> {",
    "fn is_await_for(source: &str, token: super::IdentifierToken<'_>) -> bool {",
]:
    start = source.index(signature)
    end = source.find("\nfn ", start + len(signature))
    if end == -1:
        end = len(source)
    segment = source[start:end]
    if segment.count(declaration) != 1:
        raise SystemExit(f"unused bytes declaration not found exactly once for {signature}")
    source = source[:start] + segment.replace(declaration, "", 1) + source[end:]
path.write_text(source, encoding="utf-8")
