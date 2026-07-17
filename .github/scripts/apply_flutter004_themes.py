from pathlib import Path
import base64
import gzip
import json


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one replacement in {path}, found {count}")
    file.write_text(text.replace(old, new), encoding="utf-8")


payload = json.loads(gzip.decompress(base64.b64decode('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'))) 
for path, content in payload.items():
    file = Path(path)
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(content, encoding="utf-8")

replace_once(
    "crates/dartscope-flutter/src/lib.rs",
    "mod catalogs;\nmod conventions;\n",
    "mod catalogs;\nmod conventions;\nmod themes;\n",
)
replace_once(
    "crates/dartscope-flutter/src/lib.rs",
    """pub use conventions::{
    derive_flutter_file_hints, populate_flutter_file_hints, populate_flutter_project_analysis,
};
""",
    """pub use conventions::{
    derive_flutter_file_hints, populate_flutter_file_hints, populate_flutter_project_analysis,
};
pub use themes::{
    FlutterThemeApplication, FlutterThemeApplicationKind, FlutterThemeConstruction,
    FlutterThemeConstructor, FlutterThemeFacts, derive_flutter_theme_facts,
    extract_flutter_theme_facts,
};
""",
)
replace_once(
    "docs/development/dartscope-library-plan.md",
    "- [ ] Normalize supported theme construction and application facts.\n",
    """- [x] Normalize official `ThemeData` construction and `MaterialApp`/`Theme`/`AnimatedTheme`
  application facts through a deterministic parser-independent API with source spans.
""",
)
replace_once(
    "README.md",
    """- `dartscope-flutter` derives widget, official application-route and named-navigation, asset, and
  localization conventions from generic imports, declarations, and invocations, aggregates
""",
    """- `dartscope-flutter` derives widget, official application-route, named-navigation, theme, asset,
  and localization conventions from generic imports, declarations, and invocations, aggregates
""",
)
replace_once(
    "README.md",
    """diagnostics. See [`docs/development/flutter-boundary.md`](docs/development/flutter-boundary.md) and
[`docs/development/flutter-catalogs.md`](docs/development/flutter-catalogs.md).
""",
    """diagnostics. `derive_flutter_theme_facts` and `extract_flutter_theme_facts` expose official
Material theme construction and application facts without changing the v1 inventory JSON shape.
See [`docs/development/flutter-boundary.md`](docs/development/flutter-boundary.md),
[`docs/development/flutter-catalogs.md`](docs/development/flutter-catalogs.md), and
[`docs/development/flutter-themes.md`](docs/development/flutter-themes.md).
""",
)
reference = Path("docs/reference-strategy.md")
reference_text = reference.read_text(encoding="utf-8")
section = """

## Official Flutter Theme Facts

Theme construction and application support is normative and follows the official `ThemeData`,
`MaterialApp`, `Theme`, and `AnimatedTheme` API documentation. The supported subset and explicit
non-evaluation boundary are recorded in `docs/development/flutter-themes.md`. Ecosystem theme
packages are not implied by this official support.
"""
if "## Official Flutter Theme Facts" in reference_text:
    raise SystemExit("Flutter theme reference section already exists")
reference.write_text(reference_text.rstrip() + section + "\n", encoding="utf-8")
