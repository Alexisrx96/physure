# PHS training pilot: closing the remaining Fase 0 items

## Context

An earlier session-artifact roadmap ("PHS Rollout Roadmap") laid out four phases for
introducing PHS to a non-programmer engineering team, starting with Fase 0 ("Cimientos") —
five must-haves that block the closed pilot (Fase 1) from starting. Two of the five were
resolved earlier in this same branch (originally worked in an isolated worktree, now
consolidated into `phs/correctness-and-readability`): the silent implicit-multiplication
ambiguity (`x y`) and the head-token unit/variable shadowing gap (`1/2 m v^2`), both fixed in
commit `53a89be`.

This spec covers the three remaining Fase-0 items that produce a concrete artifact: verifying
diagnostic accuracy for the two new errors, cutting a release that actually includes them, and
writing the PHS-specific tutorial that doesn't exist yet. A fourth Fase-0 item ("support
channel") is a pure process/staffing decision with no code or content shape — out of scope
here, left for the user to decide separately.

This is three pieces of different shape sharing one spec at the user's request — each section
below is independently scoped and could be executed as its own phase of the resulting plan.

## Section 1: physure-lsp diagnostic accuracy for the two new errors

### Why this needs verification, not assumption

Fase 0 originally flagged "confirm the VS Code extension surfaces line-accurate diagnostics"
as an open question. Investigation during this session found the actual mechanism: the VS
Code extension (`github.com/Alexisrx96/vsc_physure`, a separate repository not in this
workspace) gets its diagnostics, hover docs, and syntax highlighting entirely from the native
`physure-lsp` server in *this* repo — Python is only involved for the extension's
execution/live-preview/export features, not diagnostics. So the real, testable question is
narrower and answerable inside this repo: does `physure-lsp` report a correct line/column for
the two new error types.

`physure-lsp/src/incremental.rs`'s `extract_line_col_from_err` (line 383) already parses the
`"--> {line}:{col}\n..."` prefix format — which is exactly the format both of this session's
new errors use (`parser/mod.rs`'s bare-identifier-juxtaposition error and
`unit_shadowing_error`'s message). An existing test,
`test_unit_shadowing_lsp_diagnostic_location_and_cleaning` (line 800), proves this pipeline
works for the *original* (pre-session) unit-shadowing error shape. Neither of this session's
two new errors has an equivalent test — this section closes that gap.

### What to add

Two new tests in `physure-lsp/src/incremental.rs`'s `#[cfg(test)] mod tests`, mirroring
`test_unit_shadowing_lsp_diagnostic_location_and_cleaning`'s exact shape (call
`physure_script::parser::parse_phs_with_lines`, extract the error string, assert on
`extract_line_col_from_err` and `clean_error_message`):

1. A bare-identifier-juxtaposition case (`x y` with no operator between them) — assert the
   diagnostic points at the statement's line and that the cleaned message names both
   identifiers, contains neither `Generic(` nor `-->`.
2. A head-token shadowing case using the actual motivating example
   (`fn kinetic_energy(m, v) = 1/2 m v^2`) — assert the diagnostic points at the right line and
   the cleaned message names `m` and offers the `* m` / `* "m"` rewrite.

If either test reveals the diagnostic position or message is *not* accurate (e.g., points at
the wrong line, or the message is confusing), that's a real finding to report back — this
section is written as "add tests," not "assume they'll pass," per the same evidence-before-
assertion discipline used throughout this branch's other work.

## Section 2: cut release v0.2.4

### Why this is urgent, not routine

The last published tag is `v0.2.3` (2026-07-26). `git log --oneline v0.2.3..HEAD` on this
branch shows **250 commits** since then, including the entire LAB-READY milestone (loops,
concurrency, the CLI debugger, incremental LSP, compiled exports, decorators) and this
session's two correctness fixes. If the pilot installed "the latest release" today, it would
get a build that's missing the exact safety fix this spec's Section 1 verifies — a real risk,
not a process nicety.

### Process (per `CLAUDE.md`'s changelog policy and the `release` skill)

1. Confirm the last tag: `git tag -l --sort=-v:refname | head -5` (already checked: `v0.2.3`).
2. List the real commits entering this release: `git log --oneline --reverse v0.2.3..HEAD`.
3. Reconcile against `CHANGELOG.md`'s existing `[Unreleased]` section — much of the LAB-READY
   work already has entries there (written as each track landed); the task is to verify
   completeness against the actual commit list, not write from a blank page. Every entry must
   correspond to a real commit; nothing gets attributed to the release without one.
4. Move `[Unreleased]` to a new `## [0.2.4] - <date>` section with the appropriate tags line
   (`v0.2.4`, `core-v0.2.4`, and any others per the release skill's guidance on which of the
   four independent release flows apply).
5. Create a fresh, empty `[Unreleased]` section at the top.
6. Bump version numbers per the `release` skill's instructions for whichever flows apply
   (Rust core/crates.io at minimum, given the PHS work; Python/PyPI if anything in
   `physure-python` changed since `v0.2.3` too — check).

### Sequencing — do not run this from a WIP branch

Cutting a release tags a specific commit as "the" public `v0.2.4" — that should be a commit on
`main` after `phs/correctness-and-readability` has merged, not a WIP branch commit. This
section's process steps 1-6 (the `CHANGELOG.md` content work) can be prepared as a plan, but
the actual `git tag` and push — the parts that are visible to others and hard to reverse —
happen manually, after merge, with the user's explicit go-ahead.

## Section 3: the PHS-specific tutorial

### Why this is needed

`docs/tutorials/` currently holds two Jupyter notebooks (`01_best_practices.ipynb`,
`jax_tutorial.ipynb`), both about the **Python library**, not PHS the DSL. Fase 1 of the
rollout roadmap names a specific day-1 curriculum with nothing written to teach it from yet:
quantities with a unit, `=>` for conversion, `+/-` for uncertainty, `where` for showing work,
and `phs script.phs --html` as the closing deliverable.

### Format and language (decided)

- **English**, matching every other doc in the repo (`PHYSURE_LANGUAGE_GUIDE.md`,
  `physure-cli/README.md`, `INSTALL.md`) — the pilot's day-1 session can be delivered verbally
  in whatever language suits the room; the written reference stays consistent with the rest of
  the repository a future contributor would read.
- **A markdown guide plus a companion runnable `.phs` script** — not a Jupyter notebook (PHS
  isn't Python, so `.ipynb` doesn't fit the way it does for the two existing tutorials) and not
  a bare script with no prose (per Fase 1's "break it on purpose" exercise design, the reader
  needs narrative framing, not just code comments, to understand *why* an error looks the way
  it does). This mirrors `physure-cli/README.md`'s existing "language tour" structure, scaled
  up into a standalone walkthrough.

### Files

- `docs/tutorials/phs_primer.md` — the narrative guide. Structure, in order:
  1. What PHS is and isn't (one paragraph, reusing the "for the person who does the physics"
     framing from `physure-cli/README.md`) — sets expectations before any syntax.
  2. Quantities with units (`presion = 100.0 kPa`), run via `phs`.
  3. Conversion with `=>`.
  4. Uncertainty with `+/-`.
  5. `where` for showing intermediate steps.
  6. `phs script.phs --html` as the closing deliverable — what the generated report looks
     like and why it matters (the "photographed whiteboard" framing from the earlier rollout
     roadmap).
  7. The "break it on purpose" exercise: one unit-mismatch example, one bare-juxtaposition
     example (`masa velocidad` with no operator) — deliberately exercising this session's two
     fixes, so the guide teaches the exact failure mode the fixes now catch instead of letting
     it happen silently.
- `docs/tutorials/phs_primer.phs` — the companion script the reader runs alongside the guide,
  containing the worked (non-broken) version of each section's example in order, so
  `phs docs/tutorials/phs_primer.phs --html` produces one complete report matching the guide's
  narrative sequence.

### Verification

Every code block quoted in `phs_primer.md` and every statement in `phs_primer.phs` must
actually run against the `phs` binary built from this branch — no example ships unverified.
The two "break it on purpose" examples are deliberately *expected* to fail; the guide states
the exact error text the reader should see, and that text must match what `phs` actually
prints, not a guess.

## Explicitly out of scope

- The "support channel" Fase-0 item — pure process/staffing, not a code or content artifact.
- Any change to the VS Code extension itself (separate repository, not part of this workspace).
- Publishing/pushing the `v0.2.4` tag — prepared here, executed manually by the user after
  this branch merges to `main`.
- Translating `phs_primer.md`/`phs_primer.phs` into Spanish — the language decision above is
  English-only for this pass.
