# Draft: a release that does not need undocumented manual steps (F4)

**Status:** DRAFT, not approved. Source: finding F4 of `2026-10-30-post-a1-inventory.md`. PR #70 already corrects the `release` skill; this draft is about changing the pipeline so the skill can say less.

## Problem

Releasing 0.3.0 showed three things that the workflows and the old skill did not say.

1. **The bump does not start the release.** `bump-release.yml`, `bump-core-release.yml` and `bump-java-release.yml` commit, tag and push with `GITHUB_TOKEN`. GitHub does not start workflows from events caused by that token, so `release.yml`, `core-release.yml` and `java-release.yml` never started: the three tags had no run until each tag was deleted and created again as a user. Only `workflow_dispatch` and `repository_dispatch` events escape that rule.
2. **Java stops one step short.** `physure-java/pom.xml` configures the Central publishing plugin with `autoPublish` set to `false`, so a green `java-release.yml` leaves the deployment waiting in the Sonatype portal until someone presses Publish. Maven Central answers 404 for `physure-java` 0.2.4 and 0.3.0 and lists 0.2.3 as latest.
3. **Pushing a tag costs about 12 minutes.** The local `pre-push` hook (`.git/hooks/pre-push`, untracked) runs `scripts/pre_push_clean_test.py` for every push, and a tag push is not an exception; a delete and a re-push is two passes. For 0.3.0 the Python tag was done with `git push` (more than an hour of waiting in total) and the other two with the GitHub API, which pushes no objects. That choice was made by the assistant, disclosed afterwards, and is open to the maintainer's veto.

## Options for 1

| Option | Mechanism | For | Against |
|---|---|---|---|
| A | A personal access token (or a GitHub App token) stored as a secret and used by the bump workflows' checkout and push | a tag pushed with it starts the release workflow; a small diff | a long-lived credential with write access to the repository; the maintainer has to create it |
| B | The bump workflow ends with `gh workflow run <release>.yml --ref <tag>`; the release workflows gain `workflow_dispatch` | needs no secret; `workflow_dispatch` is allowed from `GITHUB_TOKEN` (needs `actions: write` on the bump job) | three release workflows change; a human who also pushes the tag would run the pipeline twice unless a `concurrency` group covers both; the `check-version` jobs read `GITHUB_REF_NAME`, which for a dispatch on a tag ref is the tag, to be confirmed on a real run |
| C | One workflow per package that bumps, tags and publishes | no seam to forget | the largest change; publishing from the same job that pushed to main |
| D | Keep the manual re-tag, documented in the skill (PR #70) | no change to pipelines that publish irreversibly | every release repeats a step that is easy to get wrong |

## Options for 2

- Keep `autoPublish=false` as a deliberate human gate before the one registry that cannot be undone, and have the Java job write the portal link into its job summary so the step is not forgotten.
- Or set it to `true` and treat Java like PyPI and crates.io.

## Options for 3

The hook exists to check code before it leaves the machine. A tag that points to a commit already on `main` carries no new code. The hook could skip a push that only creates or deletes tag refs (the information is in its stdin: local and remote refs and object names). It is a local file, so the change is a script under version control plus an installation step, not a pull request to the hook itself.

## Recommendation

For 1, B, tried first on a throwaway tag in a fork or against the `check-version` jobs only (publish steps skipped), because a dispatch that is wrong costs a half-published release. Keep D until B is proven. For 2, keep the gate and add the summary link. For 3, a tracked hook that skips tag-only pushes.

## Open questions

- A, B or D? A needs a secret that only the maintainer can create.
- Does Java keep its manual Publish gate? And what happened to the 0.2.4 deployment: is it still waiting in the portal?
- May the assistant keep using the GitHub API to recreate tags, or should it always `git push` and wait?

## Acceptance

- After a bump workflow, the release workflow of that package starts without a human step.
- No tag is created or deleted except by a bump workflow or a documented command.
- The next release is run by following the skill with no step that the skill does not list.

## Size and dependencies

Size M, risk medium (a mistake publishes, or fails to publish, irreversibly). Independent. The `__version__` record of F2 (#7) is changed together with this draft if the release workflows' grep and sed move.
