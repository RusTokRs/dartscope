#!/usr/bin/env python3
"""Patch the staged DS-CLI-003 applier for robust, diagnosable replacements."""

from pathlib import Path

APPLIER = Path("tools/ds_cli003_apply.py")

source = APPLIER.read_text(encoding="utf-8")

old_replace = '''def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")
'''
new_replace = '''REPLACEMENT_NUMBER = 0


def replace_once(path: str, old: str, new: str) -> None:
    global REPLACEMENT_NUMBER
    import re

    REPLACEMENT_NUMBER += 1
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count == 1:
        target.write_text(text.replace(old, new), encoding="utf-8")
        return
    if count > 1:
        preview = old.splitlines()[0] if old.splitlines() else repr(old)
        raise SystemExit(
            f"replacement #{REPLACEMENT_NUMBER} in {path}: "
            f"exact anchor {preview!r} matched {count} times"
        )

    pieces = re.split(r"(\\s+)", old)
    pattern = "".join(r"\\s+" if piece.isspace() else re.escape(piece) for piece in pieces)
    replaced, flexible_count = re.subn(
        pattern, lambda _match: new, text, count=2, flags=re.DOTALL
    )
    if flexible_count != 1:
        preview = old.splitlines()[0] if old.splitlines() else repr(old)
        raise SystemExit(
            f"replacement #{REPLACEMENT_NUMBER} in {path}: expected one occurrence "
            f"of {preview!r}; exact=0, whitespace-flexible={flexible_count}"
        )
    target.write_text(replaced, encoding="utf-8")
'''
if source.count(old_replace) != 1:
    raise SystemExit("original replace_once implementation was not found exactly once")
source = source.replace(old_replace, new_replace)

help_start = source.index(
    "replace_once('crates/dartscope-cli/src/main.rs', '    fn help(self) -> String"
)
help_end = source.index(
    "\nreplace_once('crates/dartscope-cli/src/main.rs', 'enum CliErrorKind", help_start
)
help_patch = '''def patch_cli_command_help() -> None:
    target = ROOT / "crates/dartscope-cli/src/main.rs"
    text = target.read_text(encoding="utf-8")
    start_marker = "    fn help(self) -> String {\\n"
    end_marker = "        format!(\\n"
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit("CLI help function start not found")
    end = text.find(end_marker, start)
    if end < 0:
        raise SystemExit("CLI help function format block not found")
    new_prefix = r\'''    fn help(self) -> String {
        let options = match self {
            Self::GraphqlContracts | Self::UriGraph => {
                "\\nOPTIONS:\\n  --env <key=value>  Add a Dart compilation-environment entry; repeatable\\n  -h, --help         Print command help"
            }
            Self::Lint => {
                "\\nOPTIONS:\\n  --config <path>        Read versioned TOML lint configuration\\n  --format <json|sarif>  Select structured output; default: json\\n  --deny-warnings        Fail when warning findings are present\\n  -h, --help             Print command help"
            }
            _ => "\\nOPTIONS:\\n  -h, --help  Print command help",
        };
\'''
    target.write_text(text[:start] + new_prefix + text[end:], encoding="utf-8")


patch_cli_command_help()
'''
source = source[:help_start] + help_patch + source[help_end:]

APPLIER.write_text(source, encoding="utf-8")
print("DS-CLI-003 applier patched")

# trigger revision 3: Node 20 staging checkout
