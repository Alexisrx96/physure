# Repo hygiene PR-2: remove noise — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete dead and misleading files from the repo root, stop tracking generated and tool-cache files in a confusing way, and keep AI planning documents out of the published docs, with no change in runtime behaviour.

**Architecture:** Plain file removals and moves plus four small config edits, each preceded by a check that nothing references the file. Verified by a strict docs build before and after, and by `cargo metadata --locked`. Design: `docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md`.

**Tech Stack:** git, mkdocs (material), `uv`, cargo.

## Conventions (apply to every task)

- Work in a new worktree off `origin/main`, branch `chore/repo-noise`. Never commit on `main`.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- `Cargo.lock` may be rewritten by `rust-analyzer` while you work. Only Task 4 commits it. Use `git add <explicit paths>`, never `-A` or `-a`, and check `git diff --cached --name-only` before each commit.
- Docs link policy: source-file links in `docs/` are GitHub blob URLs, never relative paths.
- If a check in a task fails in a way the task does not describe, stop and report; do not improvise a fix.

## Setup

- [ ] **Step 1: Create the worktree from the main checkout root**

```bash
cd /d/Projects/physure
git fetch origin
git worktree add -b chore/repo-noise .claude/worktrees/repo-noise origin/main
cd .claude/worktrees/repo-noise
git status -sb | head -1
```

Expected: `## chore/repo-noise...origin/main`. Run every later command from this worktree root.

- [ ] **Step 2: Build the docs environment and record a baseline strict build**

```bash
cd physure-python
unset VIRTUAL_ENV   # on the maintainer machine it points at the main checkout's venv
uv sync --extra docs --dev
uv run mkdocs build --strict; echo "baseline-exit=$?"
cd ..
```

Expected: `baseline-exit=0`. If `uv sync` fails because the project build needs an extra (an import error from mkdocstrings), rerun with `--all-extras`. If the baseline build fails for another reason, stop and report: later failures could not be attributed to this PR.

---

### Task 1: Remove dead root files and a tracked tool cache

**Files:**
- Delete: `unc3.py`, `unc3.rs`, `unc3.java`, `uv.lock`, `mkdocs.yml`
- Delete: `physure-python/.cache/mkdocs-jupyter/*.json` (2 files)

- [ ] **Step 1: Confirm exactly these files are tracked and unreferenced**

```bash
git ls-files | grep -E "^(unc3\.(py|rs|java)|uv\.lock|mkdocs\.yml)$"
git ls-files physure-python/.cache
ls pyproject.toml 2>&1 | head -1
grep -rnE "unc3|uv\.lock|mkdocs\.yml|\.cache/" .github Makefile physure-python/Makefile scripts physure-python/scripts .claude/settings.json | grep -v "^physure-python/uv.lock"; echo "refs-exit=$?"
grep -rn "mkdocs\.yml\|uv\.lock" README.md INSTALL.md CONTRIBUTING.md CLAUDE.md physure-python/README.md docs/*.md; echo "doc-refs-exit=$?"
```

Expected: five root file names, then two `.cache` paths, then `ls` reporting no such file (there is no root `pyproject.toml`, so the root `uv.lock` is an orphan), then `refs-exit=1` and `doc-refs-exit=1` with no output. If a doc line mentions the root `mkdocs.yml` or `uv.lock`, change that sentence to say `physure-python/mkdocs.yml` or `physure-python/uv.lock`, and include that file in this task's commit.

- [ ] **Step 2: Remove them**

```bash
git rm unc3.py unc3.rs unc3.java uv.lock mkdocs.yml
git rm -r physure-python/.cache
git status --short | head -12
```

Expected: eight deletions (`D`), nothing else besides a possibly modified `Cargo.lock` that you must not stage.

- [ ] **Step 3: Commit**

```bash
git diff --cached --name-only
git commit -m "chore: remove dead root files and a tracked mkdocs-jupyter cache" -m "unc3.{py,rs,java} are PHS compiler smoke-test output. The root uv.lock has no pyproject.toml beside it (the project lives in physure-python/). The root mkdocs.yml diverged from the one CI builds (physure-python/mkdocs.yml). The .cache files are a tool cache." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Fix `.gitignore`

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Prove the two entries are dead**

```bash
git check-ignore -v physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi; echo "ignored-exit=$?"
git ls-files physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi
```

Expected: `check-ignore` prints nothing and `ignored-exit=1` (neither file is ignored); `ls-files` lists both (they are tracked).

- [ ] **Step 2: Replace the dead block with the real cache ignore**

In `.gitignore`, replace exactly:

```
# Physure Generated Types
physure/_generated_types.py
physure/core/registry.pyi
```

with:

```
# mkdocs-jupyter execution cache
physure-python/.cache/
```

- [ ] **Step 3: Verify**

```bash
git check-ignore -v physure-python/.cache/mkdocs-jupyter/x.json
git ls-files physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi
```

Expected: the first prints a line naming `.gitignore` and the `physure-python/.cache/` pattern; the second still lists both files (they stay tracked, only the misleading ignore lines are gone).

- [ ] **Step 4: Commit**

```bash
git add .gitignore
git diff --cached --name-only
git commit -m "chore(gitignore): drop dead generated-types entries, ignore the mkdocs-jupyter cache" -m "The removed patterns were anchored at the repo root and matched nothing; the files they named live under physure-python/ and are tracked." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Mark generated files for GitHub

**Files:**
- Create: `.gitattributes`

- [ ] **Step 1: Confirm each file says it is generated**

```bash
head -1 physure-python/physure/units/core.py physure-python/physure/units/_index.py
grep -n "_generated_types.py\|registry.pyi" physure-python/physure/scripts/generate_types.py | head -3
test ! -e .gitattributes && echo "no .gitattributes yet"
```

Expected: both unit modules start with `# GENERATED CODE - DO NOT EDIT`; the generator script writes the other two; `no .gitattributes yet`. If a `.gitattributes` already exists on `origin/main`, append the lines below to it instead of creating the file.

- [ ] **Step 2: Create `.gitattributes`**

```
# Generated files: GitHub collapses their diffs in pull requests.
physure-python/physure/units/core.py linguist-generated=true
physure-python/physure/units/_index.py linguist-generated=true
physure-python/physure/_generated_types.py linguist-generated=true
physure-python/physure/core/registry.pyi linguist-generated=true
```

- [ ] **Step 3: Verify git reads it**

```bash
git check-attr linguist-generated physure-python/physure/units/core.py physure-python/physure/core/registry.pyi
```

Expected: both lines end with `linguist-generated: true`.

- [ ] **Step 4: Commit**

```bash
git add .gitattributes
git diff --cached --name-only
git commit -m "chore: mark generated files so PR diffs collapse them" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Sync the lock files with the workspace version

**Files:**
- Modify: `Cargo.lock`
- Modify: `physure-python/uv.lock`

`Cargo.toml` and `physure-python/pyproject.toml` are at the released version but the committed locks still have the previous one, so the first `cargo`, `rust-analyzer` or `uv sync` run dirties a fresh clone. Two separate commits.

- [ ] **Step 1: Show the Cargo.lock mismatch**

```bash
grep -n "^version" Cargo.toml
cargo metadata --locked --format-version 1 > /dev/null; echo "locked-exit=$?"
```

Expected: the workspace version (for example `0.2.4`), and `locked-exit=101` with a message that the lock file needs to be updated. If `locked-exit=0`, the lock is already in sync (someone fixed it): skip to Step 5.

- [ ] **Step 2: Refresh only the workspace members**

```bash
cargo update --workspace
git diff -U0 Cargo.lock | grep '^[-+]' | grep -v '^+++\|^---' | grep -vc "version = "
git diff --stat Cargo.lock
```

Expected: the count is `0` (every changed line is a `version = ` line) and the stat shows 7 insertions and 7 deletions. If anything else changed (a third-party crate version), run `git checkout -- Cargo.lock` and report.

- [ ] **Step 3: Verify the lock is now accepted as-is**

```bash
cargo metadata --locked --format-version 1 > /dev/null; echo "locked-exit=$?"
```

Expected: `locked-exit=0`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.lock
git diff --cached --name-only
git commit -m "build: sync Cargo.lock with the workspace version" -m "The release flow bumps Cargo.toml but leaves Cargo.lock behind, so any cargo or rust-analyzer run on a fresh clone dirtied the tree. Making the bump workflows refresh the lock is tracked separately." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: Show the uv.lock mismatch**

`uv sync` in this worktree may already have rewritten `physure-python/uv.lock` (version line plus a reordering of `resolution-markers`). Restore the committed file first, then check:

```bash
git checkout -- physure-python/uv.lock
cd physure-python
unset VIRTUAL_ENV
uv lock --check; echo "uv-lock-check-exit=$?"
grep -n -B1 -A2 '^name = "physure"$' uv.lock | head -8
cd ..
```

Expected: a non-zero `uv-lock-check-exit` (the lock needs an update) and the editable package entry showing `version = "0.2.3"` with `source = { editable = "." }`. If the check passes, skip to Step 8.

- [ ] **Step 6: Change only the editable package's version line**

```bash
python - <<'EOF'
import pathlib
p = pathlib.Path("physure-python/uv.lock")
raw = p.read_bytes().decode("utf-8")
eol = "\r\n" if "\r\n" in raw else "\n"
old = 'name = "physure"' + eol + 'version = "0.2.3"' + eol + 'source = { editable = "." }'
new = old.replace("0.2.3", "0.2.4")
assert raw.count(old) == 1, raw.count(old)
p.write_bytes(raw.replace(old, new).encode("utf-8"))
EOF
git diff --stat physure-python/uv.lock | tail -1
cd physure-python && uv lock --check; echo "uv-lock-check-exit=$?"; cd ..
```

Expected: `1 file changed, 1 insertion(+), 1 deletion(-)` and `uv-lock-check-exit=0`. The version string `0.2.4` must equal `version` in `physure-python/pyproject.toml`; if the workspace version has moved on, use that one. If the check still fails after this one-line change, run `cd physure-python && uv lock` instead, keep its output, and say in the commit message that the `resolution-markers` reordering comes from the uv version.

- [ ] **Step 7: Commit**

```bash
git add physure-python/uv.lock
git diff --cached --name-only
git commit -m "build: sync physure-python/uv.lock with the package version" -m "uv.lock pinned the editable physure package at the previous release, so uv sync rewrote it on every fresh clone. Only that version line changes; the reordering of resolution-markers that newer uv versions emit is left out." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: Check both locks are clean on a fresh sync**

```bash
cd physure-python && uv sync --extra docs --dev && cd ..
git status --short
```

Expected: no modified `Cargo.lock` or `physure-python/uv.lock` (a `rust-analyzer` session may still rewrite `Cargo.lock` for reasons unrelated to this PR; if so, note it). A residual `uv.lock` diff consisting only of `resolution-markers` reordering is the uv-version noise described above and is not committed.

---

### Task 5: Keep `docs/superpowers/` out of the published site

**Files:**
- Modify: `docs/language_readiness_roadmap.md` (11 links)
- Modify: `physure-python/mkdocs.yml`

`docs/language_readiness_roadmap.md` is the only page that links into `superpowers/`. Excluding the folder without rewriting those links would break `mkdocs build --strict`.

- [ ] **Step 1: Count the links that must change**

```bash
grep -c "](superpowers/" docs/language_readiness_roadmap.md
grep -rln "superpowers/" docs --include=*.md | grep -v "^docs/superpowers/"
```

Expected: `11` and exactly one file, `docs/language_readiness_roadmap.md`. Any other file listed has links to rewrite the same way.

- [ ] **Step 2: Rewrite the links to GitHub blob URLs**

```bash
sed -i 's#](superpowers/#](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/#g' docs/language_readiness_roadmap.md
grep -c "](superpowers/" docs/language_readiness_roadmap.md
grep -o "blob/main/docs/superpowers/" docs/language_readiness_roadmap.md | wc -l
```

Expected: `0`, then `11`. The visible link text (`docs/superpowers/specs/...`) stays as it is.

- [ ] **Step 3: Exclude the folder in `physure-python/mkdocs.yml`**

Replace:

```
docs_dir: ../docs
```

with:

```
docs_dir: ../docs
exclude_docs: |
  superpowers/
```

- [ ] **Step 4: Build strictly and confirm the folder is gone from the site**

```bash
cd physure-python
uv run mkdocs build --strict; echo "build-exit=$?"
test ! -e site/superpowers && echo "superpowers excluded from the site"
cd ..
```

Expected: `build-exit=0` and `superpowers excluded from the site`. If the build reports a broken link to a `superpowers/` path, a link outside `language_readiness_roadmap.md` was missed: rewrite it as in Step 2.

- [ ] **Step 5: Commit**

```bash
git add docs/language_readiness_roadmap.md physure-python/mkdocs.yml
git diff --cached --name-only
git commit -m "docs: exclude docs/superpowers from the published site" -m "The plans and specs stay in the repo where the brainstorming workflow writes them; the site no longer renders them. The 11 links from the language readiness roadmap become GitHub blob URLs, which keeps mkdocs --strict passing." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Gather the second `superpowers` folder into the first

**Files:**
- Move: 6 files `physure-python/docs/superpowers/plans/*.md` to `docs/superpowers/plans/`
- Move: 7 files `physure-python/docs/superpowers/specs/*.md` to `docs/superpowers/specs/`

`physure-python/docs/` is outside every mkdocs `docs_dir`; two locations for the same kind of document are confusing.

- [ ] **Step 1: Check for name collisions**

```bash
comm -12 <(ls physure-python/docs/superpowers/plans physure-python/docs/superpowers/specs | sort) <(ls docs/superpowers/plans docs/superpowers/specs | sort); echo "collisions-exit=$?"
git ls-files physure-python/docs | wc -l
```

Expected: no names printed (comm exit 0 with empty output is fine) and `13`. If any name is printed, stop and ask the maintainer which copy to keep.

- [ ] **Step 2: Move them**

```bash
for f in physure-python/docs/superpowers/plans/*.md; do git mv "$f" docs/superpowers/plans/; done
for f in physure-python/docs/superpowers/specs/*.md; do git mv "$f" docs/superpowers/specs/; done
find physure-python/docs -type d -empty -delete 2>/dev/null
git status --short | grep -c "^R"
ls physure-python/docs 2>&1 | head -1
```

Expected: `13` renames and `ls` reporting `No such file or directory`.

- [ ] **Step 3: Commit**

```bash
git diff --cached --name-only | head -3
git commit -m "docs: keep all superpowers plans and specs under docs/superpowers" -m "Thirteen July plans and specs lived in physure-python/docs/superpowers, outside every mkdocs docs_dir." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Verify the whole PR and open it

**Files:** none

- [ ] **Step 1: Check the final diff**

```bash
git diff --name-status origin/main...HEAD
```

Expected: `D` for `unc3.java`, `unc3.py`, `unc3.rs`, `uv.lock`, `mkdocs.yml` and the two `physure-python/.cache/...json` files; `A .gitattributes`; `M .gitignore`, `M Cargo.lock`, `M physure-python/uv.lock`, `M docs/language_readiness_roadmap.md`, `M physure-python/mkdocs.yml`; 13 `R` lines for the moved plans and specs. Nothing else.

- [ ] **Step 2: Re-run the strict docs build and the lock check**

```bash
cd physure-python && uv run mkdocs build --strict; echo "build-exit=$?"; cd ..
cargo metadata --locked --format-version 1 > /dev/null; echo "locked-exit=$?"
```

Expected: `build-exit=0` and `locked-exit=0`.

- [ ] **Step 3: Ask the maintainer to confirm the push (outward-facing)**

The pre-push hook runs a full workspace build and test and is slow. Never use `--no-verify`.

- [ ] **Step 4: Push in the background with the maximum timeout, then open the PR**

```bash
git push -u origin chore/repo-noise
gh pr create --base main --title "chore: remove dead root files, sync Cargo.lock, keep AI plans out of the docs site" --body-file - <<'EOF'
## What
- Remove `unc3.*`, the orphaned root `uv.lock`, the diverged root `mkdocs.yml` and a tracked mkdocs-jupyter cache.
- `.gitignore`: drop two dead entries, ignore the cache. `.gitattributes`: mark generated files.
- Sync `Cargo.lock` and `physure-python/uv.lock` with the released version (a fresh clone no longer dirties on the first cargo run or `uv sync`).
- `exclude_docs` for `docs/superpowers/`; the 11 links from the language readiness roadmap become GitHub blob URLs.
- Gather the 13 files of `physure-python/docs/superpowers/` into `docs/superpowers/`.

## Why
Each of these made a fresh clone confusing without doing anything: two lockfiles, two mkdocs configs, scratch output at the root, a dirty tree after the first build.

## Verification
`uv run mkdocs build --strict` before and after; `cargo metadata --locked`; grep for references to every removed file.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

Expected: the PR URL. Report it to the maintainer.
