"""Which refs a `git push` carries, as the pre-push hook sees them.

Git feeds a pre-push hook one line per ref on its standard input:

    <local ref> <local object> <remote ref> <remote object>

with the local ref ``(delete)`` when the push deletes the remote ref. The pre-push script uses
this to skip the full build-and-test run for a push that only creates or deletes tags (a release
tag, for instance), because such a push carries no new code, provided the commit the tag points
to is already on a remote branch. A tag that points to a commit the remote has never seen sends
that commit too, and then everything runs.

This module imports nothing with side effects so that a test can load it.
"""

from __future__ import annotations

import subprocess
import sys
import threading
from collections.abc import Callable, Iterable

_ZEROS = "0" * 40
_TAGS = "refs/tags/"


def commit_is_on_a_remote_branch(obj: str) -> bool:
    """True when the commit that ``obj`` names (a commit or an annotated tag) is on a remote-tracking branch."""
    result = subprocess.run(
        ["git", "branch", "-r", "--contains", f"{obj}^{{commit}}"],
        capture_output=True,
        text=True,
    )
    return result.returncode == 0 and bool(result.stdout.strip())


def only_tags_are_pushed(
    lines: Iterable[str],
    is_published: Callable[[str], bool] = commit_is_on_a_remote_branch,
) -> bool:
    """True when every ref of the push is a tag that adds no commit, or the deletion of a tag."""
    seen = False
    for line in lines:
        if not line.strip():
            continue
        parts = line.split()
        if len(parts) != 4:
            return False
        local_ref, local_obj, remote_ref, _ = parts
        if not remote_ref.startswith(_TAGS):
            return False
        if local_ref == "(delete)":
            seen = True
            continue
        if not local_ref.startswith(_TAGS) or set(local_obj) == {"0"} or not is_published(local_obj):
            return False
        seen = True
    return seen


def read_pushed_refs(timeout: float = 2.0) -> list[str] | None:
    """The lines git wrote to this process's standard input, or None when there are none to read.

    A terminal has none, and an input that stays open (an IDE, a pipe nobody closes) must not
    make the hook wait for ever, so the read gives up after ``timeout`` seconds.
    """
    if sys.stdin is None or sys.stdin.isatty():
        return None
    lines: list[str] = []

    def reader() -> None:
        try:
            lines.extend(sys.stdin.read().splitlines())
        except (OSError, ValueError):
            pass

    thread = threading.Thread(target=reader, daemon=True)
    thread.start()
    thread.join(timeout)
    return None if thread.is_alive() else lines
