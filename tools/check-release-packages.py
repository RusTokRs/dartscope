#!/usr/bin/env python3
"""Validate DartScope release metadata and build every crate archive."""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
ORDER_FILE = ROOT / "tools" / "release-crates.txt"
EXPECTED_REPOSITORY = "https://github.com/RusTokRs/dartscope"
EXPECTED_HOMEPAGE = EXPECTED_REPOSITORY


def run(*args: str, capture: bool = False) -> str:
    completed = subprocess.run(
        args,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return completed.stdout if capture else ""


def release_order() -> list[str]:
    names = [
        line.strip()
        for line in ORDER_FILE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if len(names) != len(set(names)):
        raise SystemExit("release order contains duplicate crate names")
    return names


def workspace_version() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return str(manifest["workspace"]["package"]["version"])


def validate_metadata(order: list[str], version: str) -> None:
    metadata = json.loads(
        run(
            "cargo",
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            capture=True,
        )
    )
    packages = {package["name"]: package for package in metadata["packages"]}
    if set(packages) != set(order):
        missing = sorted(set(order) - set(packages))
        extra = sorted(set(packages) - set(order))
        raise SystemExit(f"release order mismatch: missing={missing}, extra={extra}")

    positions = {name: index for index, name in enumerate(order)}
    for name in order:
        package = packages[name]
        expected_documentation = f"https://docs.rs/{name}"
        checks = {
            "version": package["version"] == version,
            "license": package["license"] == "MIT",
            "repository": package["repository"] == EXPECTED_REPOSITORY,
            "homepage": package["homepage"] == EXPECTED_HOMEPAGE,
            "documentation": package["documentation"] == expected_documentation,
            "readme": bool(package["readme"]),
            "description": bool(package["description"]),
            "keywords": bool(package["keywords"]),
            "categories": "development-tools" in package["categories"],
            "publishable": package["publish"] != [],
        }
        failed = [field for field, valid in checks.items() if not valid]
        if failed:
            raise SystemExit(f"{name} has incomplete release metadata: {failed}")
        readme = Path(package["readme"])
        if not readme.is_absolute():
            readme = Path(package["manifest_path"]).parent / readme
        if not readme.is_file():
            raise SystemExit(f"{name} readme does not exist: {readme}")

        for dependency in package["dependencies"]:
            dependency_name = dependency["name"]
            if dependency_name not in positions:
                continue
            if version not in dependency["req"] or dependency["req"] == "*":
                raise SystemExit(
                    f"{name} internal dependency {dependency_name} lacks version {version}: "
                    f"{dependency['req']}"
                )
            if positions[dependency_name] >= positions[name]:
                raise SystemExit(
                    f"publish order places {dependency_name} after consumer {name}"
                )


def package_archives(order: list[str], version: str) -> None:
    package_dir = ROOT / "target" / "package"
    shutil.rmtree(package_dir, ignore_errors=True)

    run(
        "cargo",
        "package",
        "--workspace",
        "--locked",
        "--allow-dirty",
        "--no-verify",
    )

    for name in order:
        archive = package_dir / f"{name}-{version}.crate"
        if not archive.is_file():
            raise SystemExit(f"cargo package did not create {archive}")

        prefix = f"{name}-{version}/"
        with tarfile.open(archive, "r:gz") as packaged:
            members = {member.name for member in packaged.getmembers()}
            required = {
                f"{prefix}Cargo.toml",
                f"{prefix}Cargo.toml.orig",
                f"{prefix}README.md",
            }
            missing = sorted(required - members)
            if missing:
                raise SystemExit(f"{name} archive is missing {missing}")
            normalized = packaged.extractfile(f"{prefix}Cargo.toml")
            if normalized is None:
                raise SystemExit(f"{name} archive has no normalized Cargo.toml")
            manifest_text = normalized.read().decode("utf-8")
            manifest = tomllib.loads(manifest_text)
            dependency_tables = [
                manifest.get("dependencies", {}),
                manifest.get("dev-dependencies", {}),
                manifest.get("build-dependencies", {}),
            ]
            for target in manifest.get("target", {}).values():
                dependency_tables.extend(
                    [
                        target.get("dependencies", {}),
                        target.get("dev-dependencies", {}),
                        target.get("build-dependencies", {}),
                    ]
                )
            for dependencies in dependency_tables:
                for dependency_name, dependency in dependencies.items():
                    if isinstance(dependency, dict) and "path" in dependency:
                        raise SystemExit(
                            f"{name} packaged dependency {dependency_name} still has a path"
                        )


def main() -> None:
    order = release_order()
    version = workspace_version()
    validate_metadata(order, version)
    package_archives(order, version)
    print(f"validated {len(order)} DartScope {version} package archives")


if __name__ == "__main__":
    main()
