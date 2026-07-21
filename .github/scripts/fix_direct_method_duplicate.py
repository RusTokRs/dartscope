from pathlib import Path

path = Path("crates/dartscope-index/src/navigation.rs")
text = path.read_text(encoding="utf-8")
method_marker = "\nfn resolve_method_declaration_reference("
constructor_marker = "\nfn resolve_constructor_reference("
positions = []
start = 0
while True:
    found = text.find(method_marker, start)
    if found < 0:
        break
    positions.append(found)
    start = found + len(method_marker)

if len(positions) == 1:
    raise SystemExit(0)
if len(positions) != 2:
    raise SystemExit(f"expected one or two method blocks, found {len(positions)}")

second_start = positions[1]
constructor_start = text.find(constructor_marker, second_start)
if constructor_start < 0:
    raise SystemExit("constructor resolver marker missing after duplicate method block")

path.write_text(text[:second_start] + text[constructor_start:], encoding="utf-8")
