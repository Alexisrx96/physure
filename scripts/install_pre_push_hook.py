#!/usr/bin/env python3
"""Installs the Git pre-push hook into .git/hooks/pre-push."""

import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
HOOK_FILE = REPO_ROOT / ".git" / "hooks" / "pre-push"

HOOK_SCRIPT_CONTENT = """#!/bin/sh
# Git pre-push hook for Physure
# Runs pre-push clean environment testing script before pushing

echo "Running pre-push clean environment testing script..."
python scripts/pre_push_clean_test.py
exit $?
"""


def main():
    if not (REPO_ROOT / ".git").is_dir():
        print("Error: .git directory not found. Must be run from git repository root.")
        sys.exit(1)

    hooks_dir = REPO_ROOT / ".git" / "hooks"
    hooks_dir.mkdir(parents=True, exist_ok=True)

    with open(HOOK_FILE, "w", encoding="utf-8", newline="\n") as f:
        f.write(HOOK_SCRIPT_CONTENT)

    # Make executable on Unix systems
    if hasattr(os, "chmod"):
        os.chmod(HOOK_FILE, 0o755)

    print(f"[OK] Git pre-push hook installed successfully at: {HOOK_FILE}")


if __name__ == "__main__":
    main()
