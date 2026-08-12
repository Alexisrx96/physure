# Claude Tooling Refresh (Phase 1) Implementation Plan

**For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every stale path/reference left behind by the `physure_core`/`physure` → 7-crate workspace rename, in both Claude Code tooling (`CLAUDE.md`, hooks, skills) and human docs, and add a deterministic guard so `structure.md` (the one doc that's current) can't silently drift again.

**Architecture:** Each task is a self-contained doc/config edit: confirm the stale state exists, apply the fix, verify the fix (grep or a live hook trigger), commit. No production code changes — this is entirely `.claude/`, `CLAUDE.md`, `structure.md`, and root-level docs.

**Tech Stack:** Markdown, JSON (`.claude/settings.json` hooks), bash (hook shell commands), git.

**Spec:** `docs/superpowers/specs/2026-08-11-claude-tooling-refresh-design.md`

---

### Task 1: Fix the rebuild-reminder hook path

**Files:**
- Modify: `.claude/settings.json`

- [ ] **Step 1: Confirm the hook is currently broken**

  Run: `grep -n "physure_core/src" .claude/settings.json`
  Expected: matches the `PostToolUse` hook's grep pattern (underscore), which
  does not match the real directory `physure-core/src` (hyphen) — the hook
  never fires on real edits.

- [ ] **Step 2: Fix the pattern**

  In `.claude/settings.json`, inside the `PostToolUse` → `Edit|Write` hook
  command, change:

  ```
  grep -Eq '"file_path"[[:space:]]*:[[:space:]]*"[^"]*physure_core/src'
  ```

  to:

  ```
  grep -Eq '"file_path"[[:space:]]*:[[:space:]]*"[^"]*physure-core/src'
  ```

  Full hook block after the fix:

  ```json
  {
    "hooks": {
      "PostToolUse": [
        {
          "matcher": "Edit|Write",
          "hooks": [
            {
              "type": "command",
              "command": "grep -Eq '\"file_path\"[[:space:]]*:[[:space:]]*\"[^\"]*physure-core/src' || exit 0; echo 'Rust core changed — rebuild before testing: cd physure-core && maturin develop' >&2; exit 2"
            }
          ]
        }
      ]
    }
  }
  ```

  Note the `cd physure_core` inside the echoed reminder message also needs
  the hyphen fix (`cd physure-core`) — same command string, both occurrences.

- [ ] **Step 3: Verify the hook fires**

  First confirm the edited `.claude/settings.json` is still valid JSON:
  `python -m json.tool .claude/settings.json > /dev/null && echo OK`.
  Expected: `OK` (hand-editing escaped JSON strings is easy to break).

  Then make a trivial edit to any file under `physure-core/src/` (e.g.
  append a blank line to `physure-core/src/units/mod.rs` with the Edit
  tool, then revert it). Expected: the tool response includes the stderr
  reminder "Rust core changed — rebuild before testing: cd physure-core &&
  maturin develop" and a blocked/exit-2 signal.

- [ ] **Step 4: Commit**

  ```bash
  git add .claude/settings.json
  git commit -m "fix(hooks): rebuild-reminder hook matches renamed physure-core/ path"
  ```

---

### Task 2: Add the deterministic `structure.md` staleness hook

**Files:**
- Modify: `.claude/settings.json`

- [ ] **Step 1: Confirm structure.md is already missing a real workspace member**

  Run: `grep -c "physure-wasm" structure.md`
  Expected: `0` — `physure-wasm` is a workspace member (see `Cargo.toml`)
  but isn't mentioned anywhere in `structure.md`. This is the live case the
  new hook must catch.

- [ ] **Step 2: Add the hook**

  Add a second entry to the `PostToolUse` array in `.claude/settings.json`
  (alongside the Task 1 hook), matching edits to the root `Cargo.toml`:

  ```json
  {
    "matcher": "Edit|Write",
    "hooks": [
      {
        "type": "command",
        "command": "grep -Eq '\"file_path\"[[:space:]]*:[[:space:]]*\"[^\"/]*Cargo\\.toml\"' || exit 0; missing=\"\"; for m in $(sed -n '/^members *= *\\[/,/\\]/p' Cargo.toml | grep -oE '\"[a-z0-9_-]+\"' | tr -d '\"'); do grep -q \"$m\" structure.md || missing=\"$missing $m\"; done; [ -z \"$missing\" ] && exit 0; echo \"structure.md may be missing crate(s):$missing — run the update-structure skill\" >&2; exit 2"
      }
    ]
  }
  ```

  This only fires on edits whose `file_path` ends in `Cargo.toml` at the
  workspace root (no `/` before the filename in the JSON path, so it
  excludes `physure-core/Cargo.toml` etc.). It's pure text extraction
  (`sed`/`grep`/`tr`) and list comparison — no model call, no heuristic.

  Full `PostToolUse` array after Task 1 + Task 2:

  ```json
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "grep -Eq '\"file_path\"[[:space:]]*:[[:space:]]*\"[^\"]*physure-core/src' || exit 0; echo 'Rust core changed — rebuild before testing: cd physure-core && maturin develop' >&2; exit 2"
          }
        ]
      },
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "grep -Eq '\"file_path\"[[:space:]]*:[[:space:]]*\"[^\"/]*Cargo\\.toml\"' || exit 0; missing=\"\"; for m in $(sed -n '/^members *= *\\[/,/\\]/p' Cargo.toml | grep -oE '\"[a-z0-9_-]+\"' | tr -d '\"'); do grep -q \"$m\" structure.md || missing=\"$missing $m\"; done; [ -z \"$missing\" ] && exit 0; echo \"structure.md may be missing crate(s):$missing — run the update-structure skill\" >&2; exit 2"
          }
        ]
      }
    ]
  }
  ```

- [ ] **Step 3: Verify it fires (missing case)**

  First confirm `.claude/settings.json` is still valid JSON:
  `python -m json.tool .claude/settings.json > /dev/null && echo OK`.
  Expected: `OK`. If it fails, the escaped `sed`/`grep` command string has a
  quoting mistake — fix it before proceeding (a JSON syntax error here
  disables ALL hooks, not just this one).

  Then edit root `Cargo.toml` (any trivial whitespace change counts, since
  the matcher only checks the file path, not the diff content). Expected:
  stderr includes `structure.md may be missing crate(s): "physure-wasm"`
  (or without quotes depending on the sed/grep extraction — confirm exact
  output matches a real crate name) and exit 2.

- [ ] **Step 4: Verify it stays silent (in-sync case)**

  This is naturally re-verified after Task 4 (once `structure.md` documents
  `physure-wasm`): repeat the same trivial `Cargo.toml` edit and confirm no
  hook output / exit 0.

- [ ] **Step 5: Commit**

  ```bash
  git add .claude/settings.json
  git commit -m "feat(hooks): warn when structure.md is missing a workspace crate"
  ```

---

### Task 3: Point `CLAUDE.md` at `structure.md` for architecture

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Confirm the stale content**

  Run: `grep -n "physure_core/src\|physure/domain\|physure/application\|physure/infrastructure" CLAUDE.md`
  Expected: hits at the `cd physure_core` command example (line 12) and
  throughout the "Architecture" section (lines 38-91, per the pre-fix
  version read during brainstorming).

- [ ] **Step 2: Fix the Commands section path**

  In `CLAUDE.md`, change:

  ```
  # Build Rust core (required after any change to physure_core/src/)
  cd physure_core && maturin develop && cd ..
  ```

  to:

  ```
  # Build Rust core (required after any change to physure-core/src/)
  cd physure-core && maturin develop && cd ..
  ```

  Also update the "Enable runtime beartype contracts" / test commands only
  if they reference old paths — confirm none do (Step 1's grep should show
  only the two hits above and the Architecture section).

- [ ] **Step 3: Replace the Architecture section**

  Replace the entire block from `## Architecture` through the end of the
  `### mypy plugin` subsection (i.e. everything up to but not including
  `## Philosophy & Correctness`) with:

  ```markdown
  ## Architecture

  The repo is a 7-crate Cargo workspace. **`structure.md`** (repo root) is
  the source of truth for the crate layout, module map, and how the crates
  depend on each other — read it before navigating the codebase. Keep it
  current: the `update-structure` skill audits/regenerates it, and a
  `PostToolUse` hook on the workspace `Cargo.toml` warns when a crate is
  missing from it.

  Two invariants `structure.md` documents that are worth restating here
  because they drive the code-quality policy below:

  - **`physure-core` has zero FFI dependencies** — every language binding
    (`physure-python`, `physure-wasm`, `physure-java`, `physure-cli`,
    `physure-lsp`) wraps it, never re-implements physics/unit logic.
  - **The PHS language (`physure-script`) has no fallback** — it's the one
    piece every other binding transitively depends on with no Python (or
    other) reimplementation to fall back to.
  ```

- [ ] **Step 4: Verify**

  Run: `grep -n "physure_core\|physure/domain\|physure/application\|physure/infrastructure" CLAUDE.md`
  Expected: no matches.

  Run: `grep -c "structure.md" CLAUDE.md`
  Expected: `>= 1`.

- [ ] **Step 5: Commit**

  ```bash
  git add CLAUDE.md
  git commit -m "docs: point CLAUDE.md architecture section at structure.md"
  ```

---

### Task 4: Create the `update-structure` skill and run it

**Files:**
- Create: `.claude/skills/update-structure/SKILL.md`
- Modify: `structure.md` (via running the skill)

- [ ] **Step 1: Write the skill**

  Create `.claude/skills/update-structure/SKILL.md`:

  ```markdown
  ---
  name: update-structure
  description: Audit or regenerate structure.md against the real Cargo workspace tree. Use whenever the structure.md-staleness hook fires, before a release, or whenever you suspect structure.md doesn't match the actual crates/modules.
  ---

  # Updating structure.md

  `structure.md` (repo root) is the source of truth for the workspace layout
  that `CLAUDE.md` and other docs point to. It must never silently drift the
  way `CLAUDE.md` itself did before the crate rename.

  ## Steps, in order

  1. **List the real workspace members**: `grep -A20 "^members" Cargo.toml`.
     Every member must have a corresponding `subgraph` block in
     `structure.md`'s mermaid diagram and a mention in its opening summary.

  2. **For each crate, check its actual top-level `src/` modules**:
     `ls <crate>/src/`. Compare against what `structure.md` claims for that
     crate — files renamed, removed, or added since the last update need
     their entries fixed.

  3. **Check cross-crate dependencies**: `grep '^physure-' <crate>/Cargo.toml`
     for each crate, and confirm `structure.md`'s stated "depends on X"
     relationships match.

  4. **Update `structure.md`**: fix the mermaid `flowchart` subgraphs, the
     opening prose list of crate names, and any `click` links that point at
     files that moved.

  5. **Re-verify the header note**: `structure.md` opens with "Verified
     against the actual tree" — after your edit, that claim must be true
     again. If you can't verify a section (e.g. a crate you don't have
     context on), don't guess; leave a note instead of fabricating detail.

  ## When this runs

  - Manually, whenever you suspect drift.
  - When the `PostToolUse` hook on the workspace `Cargo.toml` warns that a
    crate is missing from `structure.md`.
  ```

- [ ] **Step 2: Run the skill once, for real**

  Follow the skill's own steps against the current repo:

  ```bash
  grep -A10 "^members" Cargo.toml
  ```

  This lists all 7 members. Cross-check each against `structure.md` — the
  known gap from Task 2 Step 1 is `physure-wasm`, which has no `subgraph`
  block. Add one, following the pattern of the existing `group_java`
  subgraph (crate purpose, key files, `click` links using the same GitHub
  blob URL pattern as the other crates), and add `physure-wasm` to the
  opening "the real crates are..." sentence in `structure.md`'s header note.

- [ ] **Step 3: Verify**

  Run: `grep -c "physure-wasm" structure.md`
  Expected: `>= 1` (was `0` in Task 2 Step 1).

  Re-run Task 2 Step 3's trivial `Cargo.toml` edit: confirm the hook is now
  silent (exit 0, no stderr) — this is Task 2 Step 4's deferred check.

- [ ] **Step 4: Commit**

  ```bash
  git add .claude/skills/update-structure/SKILL.md structure.md
  git commit -m "feat: add update-structure skill, backfill missing physure-wasm entry"
  ```

---

### Task 5: Fix the `add-unit` skill

**Files:**
- Modify: `.claude/skills/add-unit/SKILL.md`

**Context:** There are two hand-maintained `physure.conf` catalogs —
`physure-core/src/units/physure.conf` (Rust, canonical) and
`physure-python/physure/infrastructure/config/physure.conf` (pure-Python,
loaded via `configparser` in `physure-python/physure/application/startup.py`
with no Rust delegation). They have already diverged (Python's copy is
missing temperature-scale units and the percent unit). Per the standing
decision that only `physure-core`'s catalog should be hand-edited going
forward (tracked separately — see the parked follow-up on reconciling/
eliminating the duplicate), this skill must point at the canonical catalog
only and flag the drift, not instruct maintaining both.

- [ ] **Step 1: Confirm current stale content**

  Run: `grep -n "physure/infrastructure" .claude/skills/add-unit/SKILL.md`
  Expected: two hits (the prose path description and the collision-check
  `grep` command), both missing the `physure-core/src/units/` prefix
  entirely — wrong crate, not just missing a hyphen.

- [ ] **Step 2: Rewrite the skill**

  Replace the full contents of `.claude/skills/add-unit/SKILL.md` with:

  ```markdown
  ---
  name: add-unit
  description: Add a unit, constant, prefix, or dimension to physure's .conf catalog. Use whenever asked to add/rename/alias a unit or physical constant — the alias-collision check and doc regeneration are mandatory steps that are easy to forget.
  ---

  # Adding a unit or constant

  All catalog entries live in `physure-core/src/units/physure.conf`
  (sections `[Dimensions]`, `[Prefixes]`, `[Units]`, `[Constants]`) — this is
  the **only** file to hand-edit. System-specific base-unit choices live in
  `physure-core/src/units/systems/international.conf` and `imperial.conf` if
  present, otherwise check `physure-core/src/units/` for the current layout.

  **Known issue:** `physure-python/physure/infrastructure/config/physure.conf`
  is a second, hand-maintained copy of this catalog that has already
  drifted out of sync with the Rust one (missing temperature-scale units and
  `%` as of 2026-08). Do not edit it to "keep it in sync" — that's a
  standing architecture problem tracked separately, not something to
  band-aid per-unit. If your change needs to show up in the pure-Python
  fallback path today, say so explicitly rather than silently patching both
  files.

  ## Formats (copy a neighboring line and adapt)

  ```ini
  # [Units] — name = factor, DIMENSION, [aliases...]
  meter    = 1.0, L, [m, meter, metro, metros]
  # Some units have extra fields (offset, noprefix, etc.) — match the nearest existing example.

  # [Constants] — name = value unit_expression
  avogadro_constant = 6.022141e+23 mol^-1
  ```

  ## Mandatory steps, in order

  1. **Collision check FIRST.** Every symbol and alias shares one namespace,
     prefixes generate more (e.g. `p` + `H` = pico-Henry vs `pH`). The
     registry only *logs a warning* on redefinition and the later
     definition silently wins — this caused the `gal` gallon/galileo bug
     (PR #17). Check every new symbol/alias:

     ```bash
     grep -rn "SYMBOL" physure-core/src/units/*.conf physure-core/src/units/systems/*.conf 2>/dev/null
     ```

     Also consider prefix + existing-symbol clashes for short symbols.

  2. **Add the entry** in `physure-core/src/units/physure.conf`, in the
     appropriate section, keeping the file's grouping/comments.

  3. **Rebuild the Rust core** (config is compiled in, not read at runtime
     for the Rust-backed path): `cd physure-core && maturin develop && cd ..`

  4. **Verify no redefinition warnings at bootstrap** and that the unit
     resolves:

     ```bash
     uv run python -c "
     import logging; logging.basicConfig(level=logging.WARNING)
     from physure import Q_
     print(Q_(1, 'NEWSYMBOL'))"
     ```

     Any `is being redefined` warning means step 1 was missed — fix before
     continuing.

  5. **Regenerate the units reference** (docs/UNITS.md is generated, never
     hand-edit) — confirm the actual generator script still exists at
     `scripts/generate_units_readme.py` (path may have moved along with the
     crate split) before running it:

     ```bash
     uv run python scripts/generate_units_readme.py
     ```

  6. **Run the tests**: `uv run pytest tests/ -x -q`. Add a conversion test
     if the unit has a nontrivial factor or offset.
  ```

- [ ] **Step 3: Verify**

  Run: `grep -n "physure-core/src/units" .claude/skills/add-unit/SKILL.md`
  Expected: at least 3 matches (prose, collision-check command, rebuild
  step).

  Run: `ls physure-core/src/units/physure.conf`
  Expected: file exists (confirms the path the skill now documents is real).

- [ ] **Step 4: Commit**

  ```bash
  git add .claude/skills/add-unit/SKILL.md
  git commit -m "fix(skills): add-unit points at physure-core (canonical), flags conf drift"
  ```

---

### Task 6: Rewrite the `release` skill for all 4 release flows

**Files:**
- Modify: `.claude/skills/release/SKILL.md`

**Context (confirmed by reading the actual workflow YAML):**

| Package | Bump workflow | Release workflow | Tag | Registry |
|---|---|---|---|---|
| `physure` (Python) | `bump-release.yml` (bumps `physure-python/physure/__init__.py`) | `release.yml` | `vX.Y.Z` | PyPI |
| `physure` (Rust crate) | `bump-core-release.yml` (bumps `[workspace.package] version` in root `Cargo.toml`) | `core-release.yml` (also publishes prebuilt `phs`/`physure-lsp` binaries to the GitHub release) | `core-vX.Y.Z` | crates.io |
| `physure-java` | `bump-java-release.yml` (bumps `physure-java/pom.xml`) | `java-release.yml` | `java-vX.Y.Z` | Maven Central |
| `physure` (npm/wasm) | **none** — `physure-wasm/Cargo.toml` inherits the workspace version (`version.workspace = true`), so it moves whenever core bumps; `physure-wasm/package.json`'s version must be bumped **by hand** to match before tagging | `wasm-release.yml` | `wasm-v X.Y.Z` | npm |

`release.yml` triggers on tags `v*`, `py-v*`, and `py-core-v*` (not just
`v*`) — document this as observed, it's a real quirk in the workflow file,
not a typo to silently "fix".

- [ ] **Step 1: Confirm current stale content**

  Run: `grep -c "bump-release.yml\|release.yml" .claude/skills/release/SKILL.md`
  Expected: shows the skill only ever mentions the two Python-release
  workflows — no mention of `core-release.yml`, `java-release.yml`, or
  `wasm-release.yml` anywhere in the file.

- [ ] **Step 2: Rewrite the skill**

  Replace the full contents of `.claude/skills/release/SKILL.md` with:

  ```markdown
  ---
  name: release
  description: Cut a release of physure (Python/PyPI, Rust core/crates.io, Java/Maven Central, or WASM/npm). Use when asked to release, publish, bump the version, or tag a version — there are four independent release flows and version numbers, each enforced by CI tag-match checks.
  ---

  # Cutting a release

  **Version bumps happen only via the bump workflows below — never by hand
  in a feature/fix PR.** Each package has its own version and its own tag
  prefix; CI rejects a release tag that doesn't match the package's
  version file.

  ## The four release flows

  | Package | Bump workflow (manual trigger) | Release workflow (tag-triggered) | Tag prefix | Registry |
  |---|---|---|---|---|
  | `physure` (Python) | `bump-release.yml` — bumps `physure-python/physure/__init__.py` | `release.yml` | `vX.Y.Z` | PyPI (Trusted Publishing) |
  | `physure` (Rust crate) | `bump-core-release.yml` — bumps `[workspace.package] version` in root `Cargo.toml` | `core-release.yml` — also builds and attaches prebuilt `phs` + `physure-lsp` binaries (5 platforms) to the GitHub release | `core-vX.Y.Z` | crates.io (Trusted Publishing) |
  | `physure-java` | `bump-java-release.yml` — bumps `physure-java/pom.xml` | `java-release.yml` — builds native JNI libs (5 platforms), publishes via Sonatype Central Portal | `java-vX.Y.Z` | Maven Central |
  | `physure` (npm/WASM) | **none — manual** | `wasm-release.yml` | `wasm-vX.Y.Z` | npm |

  No PyPI/crates.io tokens exist locally for any of these — never try
  `twine`, `cargo login`, or `npm login` locally. All four use short-lived,
  per-run tokens (OIDC Trusted Publishing for PyPI/crates.io; stored
  secrets in a required-reviewer GitHub environment for Maven/npm).

  ## Two independent versions worth knowing about

  - `physure-wasm/Cargo.toml` has `version.workspace = true` — it silently
    moves whenever `bump-core-release.yml` bumps the workspace version. It
    is **not** independently bumpable via a Cargo.toml edit alone.
  - `physure-wasm/package.json`'s `"version"` is a **separate, hand-edited**
    field. `wasm-release.yml`'s `check-version` job requires the tag,
    `Cargo.toml`'s (workspace-inherited) version, and `package.json`'s
    version to all match — so before tagging a wasm release, bump
    `physure-wasm/package.json` by hand to match whatever the current
    workspace version is.

  ## Steps: Python release

  1. Confirm main is green: `gh run list --branch main --limit 3`.
  2. Trigger the bump: `gh workflow run bump-release.yml -f bump=patch` (or
     `minor`/`major`).
  3. Watch it commit + tag + push:
     `gh run watch $(gh run list --workflow bump-release.yml --limit 1 --json databaseId -q '.[0].databaseId')`.
     This chain-triggers `release.yml`.
  4. Watch the release run: `gh run watch` (jobs: `check-version` →
     `wheels` / `sdist` → `publish` → `github-release`).
  5. Verify: `pip index versions physure` or
     https://pypi.org/project/physure/.

  ## Steps: Rust core release

  1. Confirm main is green.
  2. Trigger: `gh workflow run bump-core-release.yml -f bump=patch`.
  3. Watch it tag `core-vX.Y.Z` and push, chain-triggering `core-release.yml`.
  4. Watch the release run: jobs `check-version` → `publish` (crates.io) +
     `phs-binaries` (5-platform matrix) → `github-release`.
  5. Verify: https://crates.io/crates/physure, and check the GitHub release
     has all 5 `phs-*` binary archives attached.

  ## Steps: Java release

  1. Confirm main is green.
  2. Trigger: `gh workflow run bump-java-release.yml -f bump=patch`.
  3. Watch it tag `java-vX.Y.Z` and push, chain-triggering `java-release.yml`.
  4. Watch the release run: jobs `check-version` → `native-libs` (5-platform
     matrix) → `publish` (Maven Central via Sonatype Central Portal, GPG-signed).
  5. Verify: https://central.sonatype.com/artifact/io.github.alexisrx96/physure-java
     (propagation to Maven Central search can take a few hours).

  ## Steps: WASM/npm release

  1. Confirm main is green.
  2. Check the current workspace version: `grep -m1 '^version' Cargo.toml`.
  3. Manually bump `physure-wasm/package.json`'s `"version"` field to match
     (or to the version the workspace will be at — coordinate with a core
     release if you need a version bump first, since there's no dedicated
     wasm bump workflow).
  4. Commit that change, then tag by hand:
     ```bash
     git add physure-wasm/package.json
     git commit -m "chore: release wasm-vX.Y.Z"
     git tag wasm-vX.Y.Z
     git push origin main wasm-vX.Y.Z
     ```
  5. Watch the release run: `gh run watch` (jobs `check-version` →
     `publish`, builds `bundler` + `nodejs` targets via `wasm-pack`,
     publishes to npm).
  6. Verify: `npm view physure versions`.

  ## If it fails

  - Any `bump-*.yml` fails to push → main moved or is protected; rerun
    after rebasing, or push manually with the version it computed.
  - Any `check-version` job failure → tag doesn't match the package's
    version file; delete the bad tag (`git push origin :<tag>`), fix the
    version file, re-tag.
  - `release.yml`/`core-release.yml` publish failure mentioning trusted
    publisher / OIDC → the PyPI/crates.io Trusted Publisher config (repo,
    workflow filename, environment name) is missing or wrong — that's set
    in the registry's web UI, the user must fix it.
  - `java-release.yml` publish failure → check the `maven-central`
    environment's `CENTRAL_USERNAME`/`CENTRAL_TOKEN`/`GPG_PRIVATE_KEY`/
    `GPG_PASSPHRASE` secrets exist and the GPG key hasn't expired.
  - `wasm-release.yml` `check-version` failure → `physure-wasm/package.json`
    wasn't bumped to match the (workspace-inherited) Cargo version — fix
    and re-tag.
  - crates.io "already exists" → someone tagged `core-vX.Y.Z` for a version
    already published; bump again.
  ```

- [ ] **Step 3: Verify**

  Run: `grep -c "core-release.yml\|java-release.yml\|wasm-release.yml" .claude/skills/release/SKILL.md`
  Expected: `>= 3`.

  Cross-check every workflow filename, job name, and tag prefix named in
  the new skill against the actual files: `ls .github/workflows/`.

- [ ] **Step 4: Commit**

  ```bash
  git add .claude/skills/release/SKILL.md
  git commit -m "fix(skills): release skill covers all 4 release flows (Python/core/Java/WASM)"
  ```

---

### Task 7: Fix path references in `ROADMAP.md` and `docs/ROADMAP.md`

**Files:**
- Modify: `ROADMAP.md`
- Modify: `docs/ROADMAP.md`

**Context:** these are two genuinely different documents (not duplicates —
confirmed by diff during brainstorming) — fix paths in both independently,
do not merge them. `physure_core` (underscore) as a *lib/crate name* in
prose (e.g. `physure_core::chemistry`) is correct and must not change —
only directory-path-shaped references change.

- [ ] **Step 1: Confirm current stale content**

  Run: `grep -n "physure_core/src\|physure/domain\|physure/application" ROADMAP.md docs/ROADMAP.md`
  Expected: hits on the two subsystem-progress table rows (Core Physics,
  Uncertainty & Covariance) in both files — these use directory-path-shaped
  references (`physure_core/src/`, `physure/domain/measurement/`) that need
  the crate-rename fix. Lines referencing `physure_core::chemistry` /
  `physure_core::cas` (double-colon, Rust path syntax) are lib-name
  references, not directory paths — leave those.

- [ ] **Step 2: Fix `ROADMAP.md`**

  In `ROADMAP.md`, change:

  ```
  | **1. Core Physics & Unit Engine** | ✅ Complete | 100% | `physure_core/src/`, `physure/domain/measurement/` | Base units, dimensions, quantities, JIT/AOT |
  | **2. Uncertainty & Covariance Engine** | ✅ Complete | 95% | `physure_core/src/uncertainty/`, `physure/domain/uncertainty/` | GUM covariance, Monte Carlo, affine arithmetic |
  ```

  to:

  ```
  | **1. Core Physics & Unit Engine** | ✅ Complete | 100% | `physure-core/src/`, `physure-python/physure/domain/measurement/` | Base units, dimensions, quantities, JIT/AOT |
  | **2. Uncertainty & Covariance Engine** | ✅ Complete | 95% | `physure-core/src/uncertainty/`, `physure-python/physure/domain/uncertainty/` | GUM covariance, Monte Carlo, affine arithmetic |
  ```

- [ ] **Step 3: Fix `docs/ROADMAP.md`**

  Apply the identical change to the same two table rows in `docs/ROADMAP.md`
  (confirm line numbers with `grep -n` first, since this file's line
  numbers differ slightly from the root one per the earlier diff).

- [ ] **Step 4: Verify**

  Run: `grep -n "physure_core/src\|physure/domain\|physure/application" ROADMAP.md docs/ROADMAP.md`
  Expected: no matches.

  Run: `grep -c "physure_core::" ROADMAP.md docs/ROADMAP.md`
  Expected: unchanged from Step 1's baseline count (these lib-name
  references must survive untouched).

- [ ] **Step 5: Commit**

  ```bash
  git add ROADMAP.md docs/ROADMAP.md
  git commit -m "docs: fix stale directory paths in both ROADMAP files"
  ```

---

### Task 8: Fix the path reference in `CONTRIBUTING.md`

**Files:**
- Modify: `CONTRIBUTING.md`

- [ ] **Step 1: Confirm current stale content**

  Run: `grep -n "physure_core" CONTRIBUTING.md`
  Expected 3 hits: line ~26-27 (explains the `physure-core/` directory vs
  `physure_core` lib-name split — correct, leave as-is), line ~124
  (`physure_core/.cargo/config.toml` — this one is a directory-path
  reference and is stale), line ~176 (`physure_core::uncertainty::moments`
  — lib-name/Rust-path syntax, correct, leave as-is).

- [ ] **Step 2: Fix only the directory-path reference**

  Change:

  ```
  - **Never commit machine-specific config.** `.env`, a `physure_core/.cargo/config.toml` with
  ```

  to:

  ```
  - **Never commit machine-specific config.** `.env`, a `physure-core/.cargo/config.toml` with
  ```

- [ ] **Step 3: Verify**

  Run: `grep -n "physure_core" CONTRIBUTING.md`
  Expected: still 3 hits, but now only the two legitimate lib-name
  references (lines ~26-27 explanation, ~176 Rust path) plus confirm no
  `physure_core/.cargo` (underscore+slash) pattern remains:
  `grep -c "physure_core/" CONTRIBUTING.md` → expected `0`.

- [ ] **Step 4: Commit**

  ```bash
  git add CONTRIBUTING.md
  git commit -m "docs: fix stale physure_core/.cargo path in CONTRIBUTING.md"
  ```

---

### Task 9: Final validation sweep

**Files:** none (verification only)

- [ ] **Step 1: Repo-wide grep for the old patterns**

  ```bash
  grep -rn "physure_core/src\|physure/infrastructure\|physure/domain\|physure/application" \
    --include="*.md" --include="*.json" \
    -- CLAUDE.md CONTRIBUTING.md ROADMAP.md README.md INSTALL.md .claude/ docs/ROADMAP.md
  ```

  Expected: no matches anywhere in this file set.

- [ ] **Step 2: Confirm legitimate `physure_core` lib-name prose is untouched**

  ```bash
  grep -n "physure_core::" CONTRIBUTING.md ROADMAP.md docs/ROADMAP.md physure-core/README.md
  ```

  Expected: same hits as before Task 6-8 (nothing broken).

- [ ] **Step 3: Re-run both hooks' happy paths**

  - Edit a file under `physure-core/src/`: confirm the rebuild reminder
    still fires (Task 1).
  - Edit the root `Cargo.toml`: confirm the structure-staleness hook is now
    silent (Task 2 + 4 together).

- [ ] **Step 4: No commit** — this task is verification only. If Step 1 or
  2 finds a problem, fix it in the relevant task's file and amend that
  task's commit... no — per project convention, make a new commit instead
  of amending. Create a follow-up commit describing what was missed.
