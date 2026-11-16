"""One check for clean_worktrees.py: a merged worktree goes, one with unmerged work stays.

Run: python -m pytest scripts/test_clean_worktrees.py
"""

import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).with_name("clean_worktrees.py")


def sh(cwd, *cmd):
    subprocess.run(cmd, cwd=cwd, check=True, capture_output=True)


def test_removes_merged_keeps_unmerged(tmp_path):
    origin, repo = tmp_path / "origin.git", tmp_path / "repo"
    sh(tmp_path, "git", "init", "--bare", "-b", "main", str(origin))
    sh(tmp_path, "git", "clone", str(origin), str(repo))
    sh(repo, "git", "config", "user.email", "t@t")
    sh(repo, "git", "config", "user.name", "t")
    (repo / "a").write_text("a")
    sh(repo, "git", "add", "a")
    sh(repo, "git", "commit", "-m", "base")
    sh(repo, "git", "push", "origin", "main")
    for name in ("merged", "wip"):
        sh(repo, "git", "worktree", "add", str(tmp_path / name), "-b", name)
    (tmp_path / "wip" / "b").write_text("b")
    sh(tmp_path / "wip", "git", "add", "b")
    sh(tmp_path / "wip", "git", "commit", "-m", "unmerged work")

    out = subprocess.run(
        [sys.executable, str(SCRIPT), "--apply"],
        cwd=repo,
        capture_output=True,
        text=True,
    )
    assert out.returncode == 0, out.stderr
    assert not (tmp_path / "merged").exists()
    assert (tmp_path / "wip").exists()
    branches = subprocess.run(
        ["git", "branch"], cwd=repo, capture_output=True, text=True
    ).stdout
    assert "merged" not in branches and "wip" in branches
