#!/usr/bin/env python3
"""Brings the lock files in line with the versions of the manifests, and touches nothing else.

A version bump edits `Cargo.toml`, `pyproject.toml` or `pom.xml`. The lock files keep the old
number until the next build rewrites them, so after every release each checkout shows an
uncommitted diff of lock files. This script rewrites the `version` line of exactly two kinds of
entry:

* the workspace crates in `Cargo.lock` (their versions come from `cargo metadata --no-deps`);
* the `physure` package in `physure-python/uv.lock` (its version is the one of `pyproject.toml`).

It does not run `cargo update` or `uv lock`: those re-resolve dependencies and can reorder
markers, which a release commit must not do.

Usage: python scripts/sync_lock_versions.py [--check]

``--check`` changes nothing and exits with 1 when a lock file is out of date.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO_LOCK = ROOT / "Cargo.lock"
UV_LOCK = ROOT / "physure-python" / "uv.lock"
PYPROJECT = ROOT / "physure-python" / "pyproject.toml"

# [[package]] / name / version, for a package that has no `source` line: that is what a path
# dependency or a workspace member looks like in Cargo.lock (a registry crate has a `source`).
_CARGO_ENTRY = re.compile(
    r'(?P<head>\[\[package\]\]\r?\nname = "(?P<name>[^"]+)"\r?\nversion = ")(?P<version>[^"]+)(?P<tail>"\r?\n(?!source ))'
)
# The project itself in uv.lock is the editable entry named `physure`.
_UV_ENTRY = re.compile(
    r'(?P<head>\[\[package\]\]\r?\nname = "physure"\r?\nversion = ")(?P<version>[^"]+)(?P<tail>"\r?\nsource = \{ editable = "\." \})'
)


def rewrite_cargo_lock(text: str, versions: dict[str, str]) -> tuple[str, list[str]]:
    """Sets the version of each workspace crate in a Cargo.lock; returns the text and the names changed."""
    changed: list[str] = []

    def replace(match: re.Match[str]) -> str:
        name, current = match["name"], match["version"]
        wanted = versions.get(name)
        if wanted is None or wanted == current:
            return match[0]
        changed.append(f"{name} {current} -> {wanted}")
        return match["head"] + wanted + match["tail"]

    return _CARGO_ENTRY.sub(replace, text), changed


def rewrite_uv_lock(text: str, version: str) -> tuple[str, list[str]]:
    """Sets the version of the editable `physure` entry in a uv.lock."""
    changed: list[str] = []

    def replace(match: re.Match[str]) -> str:
        if match["version"] == version:
            return match[0]
        changed.append(f"physure {match['version']} -> {version}")
        return match["head"] + version + match["tail"]

    return _UV_ENTRY.sub(replace, text), changed


def workspace_versions() -> dict[str, str]:
    """name -> version of every workspace crate, as the manifests say."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return {package["name"]: package["version"] for package in json.loads(out)["packages"]}


def python_version() -> str:
    return tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["version"]


def sync(check_only: bool) -> int:
    """Returns the number of entries that were (or, with ``check_only``, would be) changed."""
    total = 0
    for path, rewrite in (
        (CARGO_LOCK, lambda text: rewrite_cargo_lock(text, workspace_versions())),
        (UV_LOCK, lambda text: rewrite_uv_lock(text, python_version())),
    ):
        text = path.read_bytes().decode("utf-8")
        new_text, changed = rewrite(text)
        for line in changed:
            print(f"{path.relative_to(ROOT)}: {line}")
        total += len(changed)
        if changed and not check_only:
            path.write_bytes(new_text.encode("utf-8"))
    return total


def main() -> int:
    check_only = "--check" in sys.argv[1:]
    changed = sync(check_only)
    if changed == 0:
        print("The lock files already carry the versions of the manifests.")
        return 0
    if check_only:
        print(f"{changed} lock entr{'y is' if changed == 1 else 'ies are'} out of date.")
        return 1
    print(f"{changed} lock entr{'y' if changed == 1 else 'ies'} updated.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
