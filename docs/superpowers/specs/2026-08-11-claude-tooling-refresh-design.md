# Claude tooling refresh — Phase 1: fix stale AI tooling

**Status:** approved, ready for implementation plan
**Scope:** Phase 1 of 2. Phase 2 (expand skill/agent/MCP coverage for the new crates) is a separate future spec.

## Problem

The repo grew from a two-package layout (`physure/`, `physure_core/`) into a
7-crate Cargo workspace (`physure-core`, `physure-script`, `physure-cli`,
`physure-python`, `physure-lsp`, `physure-java`, `physure-wasm`), but the
Claude Code tooling and several human docs still describe the old layout.
Concretely:

- `CLAUDE.md` describes the pre-rename architecture.
- The `PostToolUse` rebuild-reminder hook in `.claude/settings.json` matches
  `physure_core/src` (underscore) but the real directory is `physure-core/src`
  (hyphen) — the hook never fires.
- `.claude/skills/add-unit/SKILL.md` points at
  `physure/infrastructure/config/`, which no longer exists (real path:
  `physure-core/src/units/`).
- `.claude/skills/release/SKILL.md` documents 2 packages / 2 workflows; the
  repo now has 8 workflows across Python, Rust core, Java, and WASM releases.
- `ROADMAP.md`, `docs/ROADMAP.md`, and `CONTRIBUTING.md` contain stale
  directory paths in prose/tables.

`structure.md` (repo root) is the one document that's current — it already
flags `CLAUDE.md` as stale in its own header note.

## Non-goals

- Merging `ROADMAP.md` and `docs/ROADMAP.md` (different content, a content
  decision outside this fix).
- Changing where `structure.md` lives or how it's generated today — this
  spec only adds a way to keep it in sync going forward.
- Phase 2 work (new skills for wasm/java/lsp/cli publishing, subagents, MCP
  additions) — separate spec, separate approval.

## Design

### 1. `CLAUDE.md` — defer architecture to `structure.md`

Replace the "Architecture" section's layer map / object descriptions with a
short pointer to `structure.md` as the source of truth for repo layout.
Keep in `CLAUDE.md` what's genuinely its own: commands, philosophy/invariants,
code quality policy, changelog policy. Update any inline path examples
elsewhere in the file (e.g. under Commands) that still say `physure_core/`.

### 2. Fix the existing rebuild-reminder hook

`.claude/settings.json` `PostToolUse` hook: change the grep pattern from
`physure_core/src` to `physure-core/src` so it matches real edited paths.

### 3. Add a deterministic `structure.md` staleness hook

New `PostToolUse` hook, matcher `Edit|Write`, scoped to the root `Cargo.toml`:

```
1. Only proceed if the edited file is the workspace Cargo.toml (root).
2. Parse the `members = [...]` array.
3. For each member name, grep for it in structure.md.
4. If any member is missing, print a warning to stderr naming the missing
   crate(s) and pointing at the update-structure skill, exit 2.
5. Otherwise exit 0 silently.
```

Pure text/list comparison — no model call, no heuristic judgment, matches
the deterministic style of the existing rebuild hook.

### 4. New skill: `update-structure`

`.claude/skills/update-structure/SKILL.md`. Purpose: audit or regenerate
`structure.md` against the real tree when invoked (manually, or after the
hook above flags a gap). Steps:

1. List actual workspace members from root `Cargo.toml`.
2. For each crate, walk its top-level `src/` modules.
3. Diff against what `structure.md` currently documents.
4. Update the mermaid diagram and prose for anything added/removed/moved.
5. Re-verify the "verified tree" claim in `structure.md`'s own header note.

### 5. Rewrite `.claude/skills/release/SKILL.md`

Cover all 8 real workflows (`bump-release.yml`, `bump-core-release.yml`,
`bump-java-release.yml`, `release.yml`, `core-release.yml`,
`java-release.yml`, `wasm-release.yml`, `tests.yml`), which packages each
publishes, and the version-bump rules per package. Verify each documented
step against the actual workflow YAML before writing it down — don't
transcribe the old skill's assumptions.

### 6. Fix `.claude/skills/add-unit/SKILL.md`

Update the catalog path from `physure/infrastructure/config/` to
`physure-core/src/units/` (and its systems subpath), re-verify the grep
collision-check command still targets real files.

### 7. Fix path references in `ROADMAP.md`, `docs/ROADMAP.md`, `CONTRIBUTING.md`

Contextual find, not blind replace: `physure_core` (underscore) is the
correct Rust lib target name in prose (per `CONTRIBUTING.md`'s own
explanation) and must stay as-is there. Only directory/path-shaped
references get corrected to the real hyphenated paths.

## Validation

- Rebuild hook: touch a file under `physure-core/src/` via Edit, confirm the
  hook fires with the rebuild reminder.
- Structure hook: add/remove a member in the root `Cargo.toml`, confirm the
  hook fires (missing case) and stays silent (in-sync case).
- `update-structure` skill: run it once, confirm `structure.md` output
  matches the actual tree (this is also its own first real usage/test).
- `release` skill: cross-check every documented step against the current
  `.github/workflows/*.yml` contents.
- `add-unit` skill: confirm the collision-check `grep` command runs
  successfully against real files.
- Grep the repo afterward for the old stale patterns
  (`physure_core/src`, `physure/infrastructure`, `physure/domain`,
  `physure/application`) to confirm none remain outside legitimate
  Rust-lib-name prose.
