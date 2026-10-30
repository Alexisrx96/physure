# A release without undocumented manual steps (F4) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer on 2026-10-30 (the four decisions below), implemented on `chore/release-pipeline`, **not yet exercised by a real release**
**Base:** local `main` @ `da2f991`
**Follows:** `2026-10-30-post-a1-inventory.md` (finding F4); replaces the draft `2026-10-30-draft-release-pipeline-triggers.md`.

## Problem

Releasing 0.3.0 showed four things that the workflows and the old `release` skill did not say.

1. **The bump did not start the release.** `bump-release.yml`, `bump-core-release.yml` and `bump-java-release.yml` commit, tag and push with `GITHUB_TOKEN`. GitHub does not start workflows from events caused by that token (only `workflow_dispatch` and `repository_dispatch` escape the rule), so `release.yml`, `core-release.yml` and `java-release.yml` never started: each of the three tags had to be deleted and created again by a person.
2. **Java stopped one step short.** `physure-java/pom.xml` had `autoPublish` set to `false`, so a green `java-release.yml` left the deployment waiting in the Sonatype Central Portal. Maven Central answered 404 for `physure-java` 0.2.4 and 0.3.0.
3. **Pushing a tag cost about 12 minutes.** The local `pre-push` hook runs `scripts/pre_push_clean_test.py` (ruff, `cargo test`, a wheel, the Python matrix) for every push, tags included, and a delete plus a re-push is two passes.
4. **The bumps left the lock files stale.** After 0.3.0, `Cargo.lock` still said 0.2.4 for the seven workspace crates and `physure-python/uv.lock` for `physure`. The next build rewrote both, so every checkout carried an uncommitted lock-file diff until someone committed it (`50bb0ca` and `5afb14e` did for earlier releases).

## Decisions (maintainer, 2026-10-30)

| # | Question | Decision | Rejected |
|---|---|---|---|
| 1 | How does a bump start its release? | **The bump workflow dispatches the release workflow** (`gh workflow run <release>.yml --ref <tag>`), and the release workflows accept `workflow_dispatch`. | A personal access token or GitHub App secret (a long-lived credential with write access); one workflow per package that bumps, tags and publishes (the largest change, and publishing from the job that pushed to `main`); keeping the manual re-tag (every release repeats a step that is easy to get wrong). |
| 2 | Does Java keep its Publish gate? | **Publish automatically** (`autoPublish` is `true`), like PyPI and crates.io. | Keeping the gate and writing the portal link into the job summary. |
| 3 | The 12-minute hook on a tag push. | **The hook skips a push that only creates or deletes tags**, provided the tag points to a commit already on a remote branch. | Asking for `--no-verify`; using the GitHub API instead of `git push` as the only way. |
| 4 | The stale lock files. | **The bump commit carries them.** | A follow-up sync PR. |

## What was built

| Piece | Change |
|---|---|
| `bump-release.yml`, `bump-core-release.yml`, `bump-java-release.yml` | `permissions` gains `actions: write`; the last step, "Start the release pipeline", runs `gh workflow run <release>.yml --ref <tag>` with `GH_TOKEN: ${{ github.token }}`. The Python and core bumps run `scripts/sync_lock_versions.py` before committing and `git add -u -- Cargo.lock physure-python/uv.lock`. |
| `release.yml`, `core-release.yml`, `java-release.yml` | `on:` gains `workflow_dispatch:`; a `concurrency` group (`${{ github.workflow }}-${{ github.ref }}`, `cancel-in-progress: false`) queues a second run of the same tag instead of running two publications at once, and never cancels one that is already publishing. `permissions` stay `contents: read`. |
| `physure-java/pom.xml` | `<autoPublish>true</autoPublish>`, with a comment that says when and why. |
| `scripts/sync_lock_versions.py` | Rewrites only `version` lines: the workspace crates of `Cargo.lock` (names and versions from `cargo metadata --no-deps`; the entries without a `source` line) and the editable `physure` entry of `physure-python/uv.lock` (version from `pyproject.toml`). `--check` reports and changes nothing. It does not run `cargo update` or `uv lock`, which re-resolve dependencies and reorder markers. |
| `scripts/push_refs.py`, `scripts/pre_push_clean_test.py` | The hook reads the refs git writes on its standard input (`<local ref> <local object> <remote ref> <remote object>`, `(delete)` as the local ref of a deletion). When every ref is a tag, and each tag's commit is on a remote-tracking branch (`git branch -r --contains <object>^{commit}`), it prints one line and exits 0. Anything else — a branch, a tag on a commit the remote has never seen, unreadable input, a terminal — runs everything. The read gives up after two seconds so an input nobody closes cannot hang a push. |
| `.claude/skills/release/SKILL.md` | "How a release really runs" says that the bump starts the release, that the lock files travel in the bump commit, that the hook skips tag-only pushes and that Java publishes by itself. The manual tag recreation stays as the fallback, marked as such. |
| Tests | `physure-python/tests/tools/test_sync_lock_versions.py` (9) and `test_push_refs.py` (11). |

The version lines of the two lock files in the tree were brought to 0.3.0 with the script (`d14b198`): eight lines, no other change. A build leaves a larger diff because `uv` also reorders the markers of `uv.lock`, which is why the script does not run `uv lock`.

## What was measured, and what was not

Measured:

- A core bump simulated in a throwaway clone (the same shell steps as the workflow, then the script and `git add -u`) commits `Cargo.toml` (1 line), `Cargo.lock` (7) and `physure-python/uv.lock` (1): 9 insertions and 9 deletions, all version lines.
- The script, run on the real lock files of this tree, changes 8 version lines and nothing else.
- None of the three release workflows has a job conditioned on the event or on `github.ref` (`grep` for `event_name`, `if:`, `github.ref`), and every `check-version` job reads `GITHUB_REF_NAME`, which for a dispatch on a tag ref is the tag. A dispatch from a branch fails in `check-version` before anything is built.
- All eight workflow files parse; the step lists of the bump workflows were checked by hand.

**Not measured: a real dispatch.** A workflow cannot be run from a local checkout, and a failed dispatch costs a half-published release if it gets past `check-version`. Two assumptions are therefore open until the first bump after this change:

1. `GITHUB_TOKEN` with `actions: write` may dispatch another workflow on a tag ref (documented behaviour; not seen here).
2. The trusted-publisher configuration of PyPI and crates.io, which names the repository, the workflow file and the environment, accepts a `workflow_dispatch` run. None of the three changes with the event, so it should; if a registry refuses, the old recovery works (recreate the tag).

Recovery is documented in the skill: `gh workflow run <release>.yml --ref <tag>` for a missing run, and the tag recreation if the dispatch is refused. Nothing the pipeline does before the dispatch is irreversible (the bump commit and tag are the same as before).

## Consequences

- **Java is public as soon as the portal validates the deployment.** Maven Central cannot be undone. The 0.2.4 and 0.3.0 deployments were uploaded under the old setting and still have to be published (or dropped) in https://central.sonatype.com/publishing/deployments by the maintainer; nothing in this change touches them.
- **The hook is a local file.** `.git/hooks/pre-push` is untracked and only calls `scripts/pre_push_clean_test.py`, so the change reaches a checkout through the script, which is tracked. No installation step is needed where the hook already exists.
- **The bump workflows need `actions: write`**, which they did not have. It lets the job start workflows of this repository and nothing else.

## Out of scope

- `bump-wasm` (npm has no bump workflow; `wasm-release.yml` still starts from a tag pushed by hand, and `physure-wasm/package.json` is bumped by hand).
- The `__version__` record of F2, which changes together with the release workflows' `grep`/`sed` of the version if those move.
- A pull-request-based bump (a bump that goes through review). Main is not protected and the bump workflows push to it.

## Acceptance

- After a bump workflow, the release workflow of that package starts without a human step. *(Open until the next release.)*
- No tag is created or deleted except by a bump workflow or a documented command.
- A push of a tag on a published commit runs no build and no tests; a push of a branch runs everything. *(Covered by `test_push_refs.py`.)*
- A bump commit contains the manifest and the version lines of the lock files, and no other change. *(Covered by `test_sync_lock_versions.py` and the simulation above.)*
- The next release is run by following the skill with no step that the skill does not list.
