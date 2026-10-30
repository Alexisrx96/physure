"""scripts/push_refs.py: a push of tags that add no commit does not need the full pre-push run."""

import importlib.util
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[3] / "scripts" / "push_refs.py"
spec = importlib.util.spec_from_file_location("push_refs", SCRIPT)
push_refs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(push_refs)

SHA = "a" * 40
ZERO = "0" * 40


def published(_obj):
    return True


def unpublished(_obj):
    return False


def test_a_new_tag_on_a_published_commit_is_tags_only():
    line = f"refs/tags/v0.4.0 {SHA} refs/tags/v0.4.0 {ZERO}"
    assert push_refs.only_tags_are_pushed([line], published)


def test_a_tag_on_a_commit_the_remote_has_never_seen_runs_everything():
    line = f"refs/tags/v0.4.0 {SHA} refs/tags/v0.4.0 {ZERO}"
    assert not push_refs.only_tags_are_pushed([line], unpublished)


def test_deleting_a_tag_is_tags_only_and_asks_nothing_of_the_commit():
    line = f"(delete) {ZERO} refs/tags/v0.3.0 {SHA}"
    assert push_refs.only_tags_are_pushed([line], unpublished)


def test_a_branch_anywhere_in_the_push_runs_everything():
    tag = f"refs/tags/v0.4.0 {SHA} refs/tags/v0.4.0 {ZERO}"
    branch = f"refs/heads/main {SHA} refs/heads/main {ZERO}"
    assert not push_refs.only_tags_are_pushed([tag, branch], published)
    assert not push_refs.only_tags_are_pushed([branch], published)


def test_deleting_a_branch_runs_everything():
    assert not push_refs.only_tags_are_pushed(
        [f"(delete) {ZERO} refs/heads/old {SHA}"], published
    )


def test_a_branch_pushed_to_a_tag_name_runs_everything():
    # local ref is a branch, remote ref a tag: not a tag push
    assert not push_refs.only_tags_are_pushed(
        [f"refs/heads/main {SHA} refs/tags/odd {ZERO}"], published
    )


def test_no_lines_or_unreadable_lines_run_everything():
    assert not push_refs.only_tags_are_pushed([], published)
    assert not push_refs.only_tags_are_pushed(["", "  "], published)
    assert not push_refs.only_tags_are_pushed(
        ["refs/tags/v1 only-three fields"], published
    )


def test_a_tag_pointing_to_the_zero_object_runs_everything():
    assert not push_refs.only_tags_are_pushed(
        [f"refs/tags/v1 {ZERO} refs/tags/v1 {ZERO}"], published
    )


def test_this_repository_has_remote_branches_that_contain_its_own_tip():
    # the real check, on a commit that is certainly published (an ancestor of origin/main)
    import subprocess

    base = subprocess.run(
        ["git", "merge-base", "HEAD", "origin/main"],
        capture_output=True,
        text=True,
    )
    if base.returncode != 0 or not base.stdout.strip():
        return  # no remote-tracking branch in this checkout: nothing to assert
    assert push_refs.commit_is_on_a_remote_branch(base.stdout.strip())
