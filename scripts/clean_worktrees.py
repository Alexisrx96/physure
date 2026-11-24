#!/usr/bin/env python3
"""Removes the worktrees whose work is already in origin/main, with their branch and their `target/`.

Every worktree that builds owns a `target/` of 5 to 50 GB and a `.venv`, so a merged branch costs
tens of GB until its worktree is gone. A worktree is removed only when it has no uncommitted change
and every commit of its HEAD is in origin/main (`git cherry` sees no patch that is missing there,
so a merge, a rebase or a squash-by-cherry-pick all count). Anything else is listed and kept.

Usage: python scripts/clean_worktrees.py [--apply]

Without ``--apply`` it only prints what it would do. It never touches the worktree it runs from
or the main checkout, and it never deletes a branch that has a patch origin/main lacks.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def git(*args: str, cwd: str | Path | None = None) -> str:
    out = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True, encoding="utf-8"
    )
    return out.stdout.strip()


def size_gb(path: Path) -> float:
    total = 0
    for root, _, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(root, name))
            except OSError:
                pass
    return total / 1e9


def worktrees() -> list[dict[str, str]]:
    blocks = git("worktree", "list", "--porcelain").split("\n\n")
    return [
        dict(line.partition(" ")[::2] for line in b.splitlines()) for b in blocks if b
    ]


def is_merged(path: Path) -> bool:
    # `+` is a patch that origin/main does not have.
    return not any(
        line.startswith("+")
        for line in git("cherry", "origin/main", "HEAD", cwd=path).splitlines()
    )


def force_remove(path: Path) -> None:
    # ponytail: Windows "Filename too long" inside target/ -- mirror an empty dir over it, then rmdir.
    if sys.platform == "win32":
        with tempfile.TemporaryDirectory() as empty:
            subprocess.run(
                [
                    "robocopy",
                    empty,
                    str(path),
                    "/MIR",
                    "/NFL",
                    "/NDL",
                    "/NJH",
                    "/NJS",
                    "/NP",
                ],
                capture_output=True,
            )
    shutil.rmtree(path, ignore_errors=True)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument(
        "--apply", action="store_true", help="remove; the default only reports"
    )
    apply = ap.parse_args().apply

    git("fetch", "--quiet", "origin")
    here = Path(git("rev-parse", "--show-toplevel")).resolve()
    main_checkout = Path(worktrees()[0]["worktree"]).resolve()
    git("worktree", "prune")

    freed = 0.0
    for wt in worktrees():
        path = Path(wt["worktree"]).resolve()
        if path in (here, main_checkout):
            continue
        branch = wt.get("branch", "").removeprefix("refs/heads/")
        label = f"{path.name} ({branch or 'detached'})"
        if git("status", "--porcelain", cwd=path):
            print(f"keep    {label}: uncommitted changes")
        elif not is_merged(path):
            print(f"keep    {label}: has patches origin/main lacks")
        else:
            gb = size_gb(path)
            print(f"{'remove ' if apply else 'would  '} {label}: {gb:.1f} GB")
            freed += gb
            if apply:
                if subprocess.run(
                    ["git", "worktree", "remove", "--force", str(path)],
                    capture_output=True,
                ).returncode:
                    force_remove(path)
                    git("worktree", "prune")
                if branch:
                    git(
                        "branch", "-D", branch
                    )  # safe: every patch of it is in origin/main
    print(f"{'freed' if apply else 'would free'} {freed:.1f} GB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
