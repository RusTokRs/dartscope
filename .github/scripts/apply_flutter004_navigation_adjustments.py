from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one replacement in {path}, found {count}")
    file.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "crates/dartscope-flutter/tests/convention_boundary.rs",
    """    assert_eq!(inventory.routes.len(), 3);
    assert_eq!(
        inventory
            .routes
            .iter()
            .filter_map(|route| route.resolved_path.as_deref())
            .collect::<Vec<_>>(),
        ["/", "/settings", "/profile/:id"]
    );
""",
    """    assert_eq!(inventory.routes.len(), 4);
    assert_eq!(
        inventory
            .routes
            .iter()
            .filter_map(|route| route.resolved_path.as_deref())
            .collect::<Vec<_>>(),
        ["/", "/settings", "/profile/:id", "/legacy"]
    );
""",
)
replace_once(
    "crates/dartscope-flutter/tests/convention_boundary.rs",
    "    assert_eq!(project.summary.flutter_routes, 3);",
    "    assert_eq!(project.summary.flutter_routes, 4);",
)
replace_once(
    "crates/dartscope-parse/tests/fixtures/flutter_app/lib/navigation.dart",
    "          // Traditional Navigator push — not currently extracted by DartScope parser.",
    "          // Official named Navigator navigation is derived by dartscope-flutter.",
)
