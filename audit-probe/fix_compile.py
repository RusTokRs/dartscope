#!/usr/bin/env python3
"""Probe-only: neutralise the known syntax error so that later layers of the workspace can be compiled and observed."""
import pathlib

path = pathlib.Path("crates/dartscope-parse/src/pubspec_yaml_marked.rs")
source = path.read_text(encoding="utf-8")
fixed = source.replace('\\"pubspec_invalid_yaml\\"', '"pubspec_invalid_yaml"').replace(
    '\\"pubspec YAML mapping is malformed: missing key for value\\"',
    '"pubspec YAML mapping is malformed: missing key for value"',
)
path.write_text(fixed, encoding="utf-8")
print("fixed stray backslashes in pubspec_yaml_marked.rs" if fixed != source else "nothing to fix")
