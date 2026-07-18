from __future__ import annotations

from datetime import date
import importlib.util
from pathlib import Path
import sys
import tempfile
import textwrap
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "check-dependency-policy.py"
SPEC = importlib.util.spec_from_file_location("dependency_policy", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class DependencyPolicyTests(unittest.TestCase):
    def fixture(self, policy: str, audit_ignores: str = "", manifest_metadata: str = "") -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "tools").mkdir()
        (root / ".cargo").mkdir()
        (root / ".github/workflows").mkdir(parents=True)
        (root / "crates/example").mkdir(parents=True)
        (root / "tools/dependency-exceptions.toml").write_text(
            textwrap.dedent(policy), encoding="utf-8"
        )
        (root / ".cargo/audit.toml").write_text(
            f"[advisories]\nignore = [{audit_ignores}]\n", encoding="utf-8"
        )
        (root / "Cargo.toml").write_text(
            "[workspace]\nmembers = [\"crates/example\"]\n", encoding="utf-8"
        )
        (root / "crates/example/Cargo.toml").write_text(
            "[package]\nname = \"example\"\nversion = \"0.0.0\"\n" + manifest_metadata,
            encoding="utf-8",
        )
        (root / ".github/workflows/ci.yml").write_text(
            "schedule:\n  - cron: '17 6 * * 1'\n"
            "cargo install cargo-audit --version 0.22.2 --locked\n"
            "cargo install cargo-machete --version 0.9.2 --locked\n"
            "python3 tools/check-dependency-policy.py\n"
            "cargo +1.95.0 audit\n"
            "cargo +1.95.0 machete\n",
            encoding="utf-8",
        )
        return root

    def test_empty_policy_is_valid(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """
        )
        self.assertEqual(MODULE.check_policy(root, date(2026, 7, 18)), [])

    def test_exception_requires_owner_rationale_and_future_expiration(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            [[rustsec_advisory]]
            id = "RUSTSEC-2026-0001"
            owner = ""
            rationale = "short"
            expires_on = "2026-07-17"
            """,
            audit_ignores='"RUSTSEC-2026-0001"',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("owner" in message for message in messages))
        self.assertTrue(any("rationale" in message for message in messages))
        self.assertTrue(any("expired" in message for message in messages))

    def test_native_rustsec_ignore_must_match_policy(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """,
            audit_ignores='"RUSTSEC-2026-0001"',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("do not match policy" in message for message in messages))

    def test_native_machete_ignore_requires_matching_review_entry(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            """,
            manifest_metadata='\n[package.metadata.cargo-machete]\nignored = ["serde"]\n',
        )
        messages = [failure.message for failure in MODULE.check_policy(root, date(2026, 7, 18))]
        self.assertTrue(any("cargo-machete ignores" in message for message in messages))

    def test_reviewed_unused_exception_matches_native_metadata(self) -> None:
        root = self.fixture(
            """
            version = 1
            cargo_audit_version = "0.22.2"
            cargo_machete_version = "0.9.2"
            [[unused_dependency]]
            manifest = "crates/example/Cargo.toml"
            dependency = "serde"
            owner = "@maintainer"
            rationale = "Used only by generated source checked in during release validation."
            expires_on = "2026-08-01"
            """,
            manifest_metadata='\n[package.metadata.cargo-machete]\nignored = ["serde"]\n',
        )
        self.assertEqual(MODULE.check_policy(root, date(2026, 7, 18)), [])


if __name__ == "__main__":
    unittest.main()
