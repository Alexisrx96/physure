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
   Also check the reverse direction: every `subgraph` block already in
   `structure.md` must correspond to a member still listed in `Cargo.toml`
   — if a crate was removed from the workspace, remove or flag its
   now-orphaned subgraph rather than leaving it behind.

2. **For each crate, check its actual top-level `src/` modules**:
   `ls <crate>/src/`. Compare against what `structure.md` claims for that
   crate — files renamed, removed, or added since the last update need
   their entries fixed. While you're in there, re-read the crate's
   one-line purpose description in `structure.md` against what its code
   actually does now — a crate's role can drift even when its file list
   doesn't, leaving a stale purpose blurb behind.

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
