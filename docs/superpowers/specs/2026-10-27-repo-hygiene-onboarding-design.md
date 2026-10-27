# Repo hygiene and contributor onboarding — design

**Date:** 2026-10-27
**Status:** design approved by the maintainer; spec awaiting review
**Base:** `origin/main` @ `755ae97` (local `main` is 4 release commits behind and is not touched)

## Goal

Cut the incidental complexity a new contributor hits between `git clone` and
a first green test, and make the documentation agree with the code.

Success looks like: one documented command sequence gets a fresh clone to
green tests, every command in `CLAUDE.md` / `CONTRIBUTING.md` actually runs,
and no document contradicts what the code does.

## Out of scope (separate specs)

- **A1 — Python-side unit/dimension algebra duplicating Rust.** `physure-python/physure/domain/measurement`
  keeps its own `Dimension` / `CompoundUnit` / `UnitSystem` (dict-of-exponents, ~7.2k lines)
  and mirrors every registration into the Rust `UnitRegistry`. Making Python a thin layer
  over `physure-core` is a multi-week refactor and gets its own design.
- **A2 — layer names that do not hold.** `application/`, `domain/`, `core/` import each
  other in both directions (14 `domain → application` imports, `core → domain` imports,
  213 function-local imports, 11 `noqa: E402`).
- **A3 — string-typed backend dispatch** (`"physure._core.Quantity" in str(type(x))` in
  `core/dispatcher.py` and `domain/measurement/quantity.py`).
- **A4 — duplicate CAS** (sympy-based Python vs `physure-script/src/symbolic`) and
  duplicate plotting code.
- **The ~11k lines of generated unit modules** (`units/core.py`, `units/_index.py`):
  removing them depends on A1.
- **Release workflow dedupe** (3 near-identical `bump-*.yml`) and making the bump flow
  refresh `Cargo.lock`: cannot be tested locally.
- **Scope reduction** (4 transpilers, `phs` self-upgrade / packaging / web server):
  a maintainer decision, not hygiene.

## Evidence (measured on `origin/main`)

| Fact | How it was checked |
|---|---|
| No `pyproject.toml` at the repo root, yet root `uv.lock` (941 KB) is tracked and the root `Makefile` runs `uv run pytest` / `uv run ruff` | `ls`, `git ls-files`, `Makefile` |
| `CLAUDE.md` says `uv sync` at the root and `cd physure-core && maturin develop`; `CONTRIBUTING.md` says everything runs from `physure-python/` | read both |
| `CONTRIBUTING.md` says "six crates"; the workspace has 7 (`physure-wasm` missing) | `Cargo.toml` |
| `structure.md` says "Everything has a pure-Python fallback except `repl.py`"; `physure/__init__.py` raises `ImportError` without `physure._core`, so no such fallback exists | `__init__.py` |
| `structure.md` lists `parser.rs`; it is now `parser/`, built on `phs.pest`. `PhsLexer` is only used by the CAS parser (`symbolic/parser.rs`) | `ls`, grep |
| Two `mkdocs.yml`; CI builds only `physure-python/mkdocs.yml` (`docs_dir: ../docs`). The root copy diverged (different description, nav, `docs_dir`) | `diff`, `tests.yml` |
| `physure-core/src/units/physure.conf` (683 lines) and `physure-python/.../config/physure.conf` (656 lines) differ in 45 lines (both are LF in the index; the CRLF seen in a Windows working tree is `autocrlf`). `Q_(1, "percent")`, `"ppm"` and the Spanish alias `julio` raise `UnknownUnitError` in Python, and the Python copy lacks `parallel_threshold`, `max_loop_iterations`, `max_pipeline_steps`. Root cause of the missing `percent`: its `[%, percent, ...]` alias list makes `configparser`'s default interpolation raise `InterpolationSyntaxError`; with `interpolation=None` the core file parses (98 units). (`celsius`/`fahrenheit` do resolve in Python although its copy does not define them; their provenance is a baseline question for PR-3.) | `diff`, `configparser` experiment, `Q_(...)` probe |
| `ROADMAP.md` (110 lines) and `docs/ROADMAP.md` (112 lines) differ | `diff -q` |
| `unc3.{py,rs,java}` (PHS compiler output) are tracked at the root; commit `f4ec31c` already removes them on another branch | `git ls-tree`, `git log --all` |
| `physure-python/.cache/mkdocs-jupyter` (2 files) is tracked | `git ls-tree` |
| `.gitignore` lists `physure/_generated_types.py` and `physure/core/registry.pyi`, but the real files live under `physure-python/physure/`, are tracked, and `git check-ignore` reports them as **not ignored** | `git check-ignore` |
| `physure-python/Cargo.toml` says `python-source = "python"`; `pyproject.toml` says `"."`; there is no `python/` directory | read both |
| `Cargo.toml` says `0.2.4`, but the committed `Cargo.lock` still says `0.2.3` for all 7 workspace crates; the first `cargo` or `rust-analyzer` run rewrites it and leaves a fresh clone with a dirty tree | `git diff` after rust-analyzer touched a clean worktree |
| 28 AI-generated plans/specs sit in `docs/superpowers/` (inside the published `docs_dir`, no `exclude_docs`); another 13 sit in `physure-python/docs/superpowers/` (outside any `docs_dir`) | `git ls-files`, `mkdocs.yml` |

Nothing outside CI references the root `uv.lock`, `unc3.*`, `.cache/`, or the root
`mkdocs.yml` (grep over `.github`, both `Makefile`s, `scripts/`, `.claude/settings.json`).

## Design

Three PRs, each one topic, each verifiable and revertible on its own. Every PR is
developed in its own worktree branch off `origin/main`; nothing is committed to `main`.
Conventional commits, `CHANGELOG.md` `[Unreleased]` entry only where a library user can
observe a change (PR-3). The 12 untracked WIP documents in the main checkout are left alone.

### PR-1 — documentation tells the truth (docs only)

1. **`CLAUDE.md` Commands:** run `uv sync`, `uv run pytest`, `ruff`, `ty` from `physure-python/`;
   build with `cd physure-python && uv run maturin develop --release`.
2. **`CONTRIBUTING.md`:** "six" → seven crates and add a `physure-wasm` row (npm package
   `physure`); add `cd physure-python && uv run mkdocs serve` to Build; add one line that
   inside a git worktree you must check which venv is active before `maturin develop`.
3. **`structure.md`:** regenerate with the `update-structure` skill, and state reality:
   the native extension is mandatory; Python keeps its own unit/dimension algebra mirrored
   into the Rust registry (target of A1); the DDD-style folder names do not describe the
   import graph (target of A2); `parser/` + `phs.pest`; `PhsLexer` scope.
4. **Root `Makefile`:** Python targets become `cd physure-python && uv run ...`;
   `cargo test --workspace` stays at the root; `sonar` unchanged.
5. **Roadmaps:** `ROADMAP.md` and `docs/ROADMAP.md` differ in link base (`docs/x.md` for
   GitHub, `x.md` for mkdocs), in one "Active execution plan" line, and in the timeline
   block, where each file has items the other lacks. Maintainer decision: combine both
   and remove nothing from the root file. The timeline becomes the union of both and is
   written to both files; neither file becomes a pointer. Moving the five per-area
   roadmaps is dropped: ~15 relative links plus the nav would change for no gain.

Docs link policy applies: no relative links to source files, both ends of any
doc-to-doc link committed together.

### PR-2 — remove noise (no behaviour change)

1. Remove `unc3.{py,rs,java}` with `git rm` (same effect as `f4ec31c` on `wip/consolidated`,
   without importing a commit from a WIP branch).
2. Remove root `uv.lock`. Keep `physure-python/uv.lock`.
3. Remove root `mkdocs.yml`. CI's `physure-python/mkdocs.yml` is the live one.
4. Untrack `physure-python/.cache/mkdocs-jupyter` and ignore `physure-python/.cache/`.
5. `.gitignore`: delete the two dead entries (`physure/_generated_types.py`,
   `physure/core/registry.pyi`). The files stay tracked; they are produced by
   `physure/scripts/generate_types.py`.
6. New `.gitattributes` marking `linguist-generated=true` on `units/core.py`,
   `units/_index.py` (both carry a "GENERATED CODE - DO NOT EDIT" header),
   `_generated_types.py` and `core/registry.pyi` so GitHub collapses their diffs.
7. `exclude_docs: superpowers/` in `physure-python/mkdocs.yml`. The specs stay where they
   are, so the brainstorming/writing-plans workflow keeps working. `docs/language_readiness_roadmap.md`
   links to 11 files under `superpowers/` with relative paths; excluding them would break
   `mkdocs build --strict`, so those links become GitHub blob URLs, the repo's convention.
8. Commit the `Cargo.lock` sync (7 `version = "0.2.3"` → `"0.2.4"` lines), so a fresh clone
   builds with a clean tree. The root cause, the `bump-*.yml` flow not refreshing the lock,
   is part of the deferred release-workflow item.
9. Move the 13 files in `physure-python/docs/superpowers/` into `docs/superpowers/` with
   `git mv`, **only if no file name collides**; on a collision, stop and ask.

### PR-3 — single source of truth for config and build metadata (touches the build)

1. **One `physure.conf`.** `physure-core` owns the catalog; Python consumes it. Python-only
   system files (`systems/imperial.conf`, `systems/international.conf`) stay. The mechanism needs a short design inside the
   plan; candidates, in order of preference:
   (a) expose the embedded catalog from `_core` and have `application/startup.py` read it;
   (b) copy the core file into the package at build time and gitignore the copy.
   A symlink is excluded (Windows contributors). **Chosen: (a).** `physure-core` already
   exposes `units::conf::DEFAULT_PHYSURE_CONF` (an `include_str!`), so it is one
   `#[pyfunction] default_conf()` plus a `.pyi` line; (b) adds a build step.
   Python's `ConfigParser` must be created with `interpolation=None` (the catalog contains
   a literal `%`), and `scripts/compile_units.py` / `scripts/generate_units_readme.py`, which
   hard-code the Python copy's path, read the core file instead.
   Constraints: first-use stays within the ~0.5 s budget; `clean_state` fixture still
   resets everything; generated unit modules and `docs/UNITS.md` are regenerated (large
   diff, reviewed by regenerating, not by reading). Python gains `percent`, `ppm`,
   `julio`, etc., which is the intended behaviour and gets a `CHANGELOG` entry. The
   `add-unit` skill and `structure.md` stop describing the copy as a known drift.
   PR-3 starts after PR-2 has landed (it needs the `Cargo.lock` sync to keep its own
   lock diff clean).
2. **Maturin:** delete the stale `[package.metadata.maturin]` table in
   `physure-python/Cargo.toml`; `pyproject.toml`'s `[tool.maturin]` is authoritative.
3. **`physure-python/Cargo.toml`:** drop the "re-export workspace deps" block, but only
   the lines a `cargo build -p physure-python` proves unneeded. `src/lib.rs` references no
   `nalgebra`, `chrono`, `sprs`, `rand`, `rand_distr`, `dyn_clone`, `ndarray` or `arrow` crate
   (the only hits are a doc comment and function names); `num-rational`, `num-traits`,
   `numpy`, `pyo3` and the two workspace crates stay.
4. **Root `Cargo.toml`:** reword the `[profile.release]` comment that says "both crates".
   The shared `lib.name = "physure_core"` in `physure-core` (rlib) and `physure-python`
   (cdylib) builds today; it is left alone and gets a comment saying so. PyO3's module name
   comes from `#[pymodule(name = "_core")]`, not from that field.

## Verification

| PR | Checks |
|---|---|
| PR-1 | `cd physure-python && uv run mkdocs build --strict`; every command in the edited docs executed verbatim in a fresh worktree; `update-structure` audit clean; before/after reproduction of the root `Makefile` failure |
| PR-2 | grep for remaining references to each removed file; `mkdocs build --strict`; built site contains no `superpowers/` pages; CI green |
| PR-3 | `cargo test -p physure -p physure-script -p physure-cli -p physure-lsp`; `cargo build -p physure-python`; `maturin develop --release` from `physure-python/` with the active venv verified before and after; full `uv run pytest` including doctests, coverage ≥ 80%; `ruff check` + `ruff format --check`; `time python -m physure "500 N / 2 m^2 => kPa"` within ~0.5 s; wheel file list compared before/after |

## Risks

- The pre-push hook runs a full workspace build and test: push in the background with a long
  timeout; never `--no-verify`.
- `maturin develop` from a worktree can install into another checkout's venv.
- PR-3 regenerates large generated files; any hand edit there is lost.
- `exclude_docs` needs mkdocs ≥ 1.5 (`mkdocs-material>=9.5` already implies it).

## Decisions left to the plan

- Roadmap consolidation: exact moves, approved by the maintainer.
- PR-3 mechanism for the single `.conf`: (a) or (b), chosen after reading `startup.py`.
