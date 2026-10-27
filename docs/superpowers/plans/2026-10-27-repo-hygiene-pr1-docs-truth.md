# Repo hygiene PR-1: documentation tells the truth — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every command and claim in `CLAUDE.md`, `CONTRIBUTING.md`, `structure.md` and the root `Makefile` match what the code does.

**Architecture:** Docs-only. A handful of exact-text edits, each checked by running the documented command or by grep, then one strict docs build. Design: `docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md`.

**Tech Stack:** Markdown, GNU make, `uv`, mkdocs (material), git.

## Conventions (apply to every task)

- Work only in the worktree `D:\Projects\physure\.claude\worktrees\repo-hygiene`, branch `docs/repo-hygiene-onboarding`. It already holds the spec commit. Never commit on `main`.
- `Cargo.lock` may show as modified in this worktree: `rust-analyzer` rewrites it because `origin/main`'s lock is stale (PR-2 fixes that). **Never use `git add -A` or `git commit -a`.** Always `git add <explicit paths>` and check `git diff --cached --name-only` before committing.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Docs link policy: no relative links to source files; if a doc links to another doc, both are committed together. This plan adds no new links.
- `make` is not installed on the maintainer's Windows machine. Recipes are verified by running their command lines by hand.

---

### Task 1: Fix the commands in `CLAUDE.md`

**Files:**
- Modify: `CLAUDE.md` (the `## Commands` code block, first lines)

- [ ] **Step 1: Reproduce the wrong commands**

Run from the worktree root:

```bash
uv sync --all-extras; echo "exit=$?"
test -f physure-core/pyproject.toml && echo "pyproject in physure-core: yes" || echo "pyproject in physure-core: NO"
test -f pyproject.toml && echo "pyproject at root: yes" || echo "pyproject at root: NO"
```

Expected: `uv` fails with an error about not finding a `pyproject.toml` (`exit=2`), then `NO` twice. This proves the documented `uv sync` and `cd physure-core && maturin develop` cannot work.

- [ ] **Step 2: Edit `CLAUDE.md`**

Replace this exact text:

```
# Install deps (including dev)
uv sync --all-extras

# Build Rust core (required after any change to physure-core/src/)
cd physure-core && maturin develop && cd ..
```

with:

```
# Every Python command below runs from physure-python/ — the virtualenv and
# pyproject.toml live there, not at the repository root.
cd physure-python

# Install deps (including dev)
uv sync --all-extras --dev

# Build the Rust extension (required after any change under physure-core/src/,
# physure-script/src/ or physure-python/src/)
uv run maturin develop --release
```

- [ ] **Step 3: Verify the old commands are gone**

```bash
grep -c "cd physure-core && maturin" CLAUDE.md
grep -n "uv sync\|maturin develop" CLAUDE.md
```

Expected: first command prints `0`; second shows only the two new lines (`uv sync --all-extras --dev` and `uv run maturin develop --release`).

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git diff --cached --name-only
git commit -m "docs(claude): run Python commands from physure-python/, fix maturin invocation" -m "CLAUDE.md told readers to run uv at the repo root (there is no pyproject.toml there) and maturin from physure-core/ (a pure-Rust crate). CONTRIBUTING.md already had it right." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Fix `CONTRIBUTING.md` (crate count, wasm row, docs and worktree notes)

**Files:**
- Modify: `CONTRIBUTING.md` (Layout intro, crate table, Build section)

- [ ] **Step 1: Confirm the facts**

```bash
sed -n '/^members/,/^]/p' Cargo.toml | grep -c '"physure-'
grep -n "six crates" CONTRIBUTING.md
grep -c "physure-wasm" CONTRIBUTING.md
```

Expected: `7`, a line number for "six crates", and `0`.

- [ ] **Step 2: Edit the Layout intro**

Replace:

```
One repository, six crates and one Python package. The names do not all match their
directories, which trips people up:
```

with:

```
One repository, seven workspace crates (`physure-python/` is both the PyO3 crate and the
Python package). The names do not all match their directories, which trips people up:
```

- [ ] **Step 3: Add the `physure-wasm` row**

Replace:

```
| `physure-lsp/` | The language server for PHS | crate `physure-lsp` |
```

with:

```
| `physure-lsp/` | The language server for PHS | crate `physure-lsp` |
| `physure-wasm/` | The `wasm-bindgen` bindings for JS/TS | npm package **`physure`** |
```

- [ ] **Step 4: Add the docs-preview and worktree notes**

Replace this exact line:

```
for `target/release/phs` — rebuild it before trusting what the CLI prints.
```

with:

```
for `target/release/phs` — rebuild it before trusting what the CLI prints.

Preview the documentation site with `cd physure-python && uv run mkdocs serve`. CI builds it
with `uv run mkdocs build --strict` from the same directory.

Working in a git worktree? Run `uv run maturin develop --release` from that worktree's own
`physure-python/`. If you ever run `maturin` without `uv run`, check `echo $VIRTUAL_ENV`
first: a venv inherited from another checkout would receive your build silently.
```

- [ ] **Step 5: Verify**

```bash
grep -n "seven workspace crates\|physure-wasm\|mkdocs serve\|worktree's own" CONTRIBUTING.md
```

Expected: four matching lines.

- [ ] **Step 6: Commit**

```bash
git add CONTRIBUTING.md
git diff --cached --name-only
git commit -m "docs(contributing): seven crates incl. physure-wasm, docs preview and worktree venv note" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Make `structure.md` describe the real code

**Files:**
- Modify: `structure.md` (lines 35, 47, 109, 134, 184, 186)

- [ ] **Step 1: Prove each claim is stale**

```bash
ls physure-script/src/parser.rs physure-script/src/interpreter.rs 2>&1 | head -2
grep -rn "PhsLexer" physure-script/src --include=*.rs | grep -v "^physure-script/src/lexer.rs" | cut -d: -f1 | sort -u
grep -n "raise ImportError\|^    raise" physure-python/physure/__init__.py | head -2
```

Expected: both `ls` targets report `No such file`; inside `physure-script`, `PhsLexer` is used only from `src/lib.rs` (re-export) and `src/symbolic/parser.rs` (the PyO3 crate also exposes it as `_core.tokenize_phs_expression`); `__init__.py` raises `ImportError` when the extension is missing.

- [ ] **Step 2: Fix the PHS engine node (line 35)**

Replace:

```
  node_phs_engine{{"lexer.rs · parser.rs · ast.rs ·<br/>resolver.rs · interpreter.rs<br/>PhsLexer/PhsParser/PhsInterpreter/eval_phs<br/>[interpreter.rs]"}}
```

with:

```
  node_phs_engine{{"phs.pest · parser/ · ast.rs ·<br/>resolver.rs · interpreter/<br/>PhsParser/PhsInterpreter/eval_phs<br/>[interpreter/mod.rs]"}}
```

- [ ] **Step 3: Fix its click target (line 134)**

Replace:

```
click node_phs_engine "https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter.rs"
```

with:

```
click node_phs_engine "https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter/mod.rs"
```

- [ ] **Step 4: Fix the REPL node and edge (lines 47 and 109)**

Replace:

```
  node_repl["repl.py<br/>PHS REPL (python -m physure) —<br/>the ONE feature with no Python fallback<br/>[repl.py]"]
```

with:

```
  node_repl["repl.py<br/>PHS REPL (python -m physure) —<br/>evaluates PHS through the native engine<br/>[repl.py]"]
```

Replace:

```
node_repl -->|"requires (no Python fallback)"| node_pyo3
```

with:

```
node_repl -->|"evaluates PHS via"| node_pyo3
```

- [ ] **Step 5: Fix the `physure-script` row (line 184)**

Replace the substring:

```
lexer → parser → resolver → tree-walking interpreter, a small CAS (`symbolic/`)
```

with:

```
pest grammar (`phs.pest`) → `parser/` → resolver → tree-walking interpreter, a small CAS (`symbolic/`, tokenised by `lexer.rs`, which the language parser does not use)
```

- [ ] **Step 6: Fix the Python row (line 186)**

Replace the substring:

```
Everything has a pure-Python fallback **except** `repl.py` (PHS evaluation), which hard-requires the native extension.
```

with:

```
The native extension is mandatory (`physure/__init__.py` raises `ImportError` without it). Python still keeps its own unit/dimension algebra (`domain/measurement`: `Dimension`, `CompoundUnit`, `UnitSystem`; the `configparser`-based `.conf` loader is `UnitSystemBuilder` in `application/startup.py`) and also mirrors each unit into the Rust `UnitRegistry` on a best-effort basis (registration errors there are suppressed) — a known duplication with its own follow-up design (item A1 of the repo-hygiene spec). The folder names `application/`, `domain/` and `core/` are historical: they import each other in both directions.
```

- [ ] **Step 7: Verify**

```bash
grep -n "pure-Python fallback\|no Python fallback\|ONE feature\|lexer → parser\|src/interpreter.rs" structure.md; echo "stale-matches-exit=$?"
```

Expected: no matching lines and `stale-matches-exit=1`.

- [ ] **Step 8: Run the audit skill**

Invoke the `update-structure` skill. Expected: it reports no drift in the parts edited above. If it proposes edits elsewhere, apply only those that `ls` or `Cargo.toml` prove.

- [ ] **Step 9: Commit**

```bash
git add structure.md
git diff --cached --name-only
git commit -m "docs(structure): describe the real parser layout and the mandatory native extension" -m "structure.md claimed a pure-Python fallback that physure/__init__.py does not have, listed parser.rs and interpreter.rs (both are directories now) and omitted phs.pest. It now also names the Python-side unit algebra duplication." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Make the root `Makefile` work

**Files:**
- Modify: `Makefile`

Recipe lines start with a TAB character, not spaces.

- [ ] **Step 1: Reproduce the failure**

```bash
uv run pytest --collect-only -q; echo "exit=$?"
```

Expected: fails with a non-zero exit. `uv` runs "outside of a project" (there is no `pyproject.toml` at the root) and pytest then cannot import `physure._core`. This is what `make test` runs today.

- [ ] **Step 2: Rewrite the Makefile**

Replace the whole file with (recipe lines TAB-indented):

```make
.PHONY: sonar test lint format help

help:
	@echo "Available commands:"
	@echo "  make sonar    - Run pytest coverage, Rust lcov, and submit SonarQube analysis"
	@echo "  make test     - Run Python and Rust test suites"
	@echo "  make lint     - Run ruff check and ruff format check"
	@echo "  make format   - Auto-format code using ruff"

sonar:
	python3 scripts/run_sonar.py

# The Python project (pyproject.toml, .venv) lives in physure-python/.
test:
	cd physure-python && uv run pytest
	cargo test --workspace

lint:
	cd physure-python && uv run ruff check .
	cd physure-python && uv run ruff format --check .

format:
	cd physure-python && uv run ruff format .
```

- [ ] **Step 3: Check tabs and run the recipe lines by hand**

```bash
grep -nP "^ +(cd|cargo|python3)" Makefile; echo "space-indented-recipes-exit=$?"
cd physure-python && uv run pytest --collect-only -q | tail -2; cd ..
```

Expected: first command prints nothing and `exit=1` (no space-indented recipes). The second needs the environment from Task 7; if it is not built yet, defer this line to Task 7 Step 3.

- [ ] **Step 4: Commit**

```bash
git add Makefile
git diff --cached --name-only
git commit -m "build(make): run Python targets from physure-python/" -m "make test/lint/format ran uv at the repo root, where there is no pyproject.toml." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Merge the two roadmap timelines (maintainer decision recorded)

**Files:**
- Modify: `ROADMAP.md` (timeline block only)
- Modify: `docs/ROADMAP.md` (timeline block only)

Maintainer decision (2026-10-27): combine both timelines and do not remove anything from the root file. So the four `timeline` lines become the union of both files' items and are written to **both** files. Neither file is replaced by a pointer; each keeps its own link base (`docs/x.md` on GitHub, `x.md` for mkdocs) and every other difference. The only wording change is that the docs copy's "Chemistry Extension (Species & Reactions)" becomes the root copy's "Chemistry Python Extension (Species & Reactions)" (same item, the more specific name).

- [ ] **Step 1: Show the difference being merged**

```bash
diff ROADMAP.md docs/ROADMAP.md | grep -n "Q3 2026\|Q4 2026\|Q1 2027\|Q2 2027"
```

Expected: eight timeline lines (four from each file).

- [ ] **Step 2: Write the merged timeline into `ROADMAP.md`**

In `ROADMAP.md`, replace these four lines:

```
    Q3 2026 (Completed) : Rust Core Engine : Correlated Uncertainty & Covariance : Chemistry Python Extension (Species & Reactions) : Native Symbolic AST & Regression
    Q4 2026 (In Progress) : Multivariate Polynomials & Gröbner Bases (F4/F5) : Rust Core Chemistry Module (physure_core::chemistry) : Risch Integration Engine : Buckingham Pi Nullspace Solver
    Q1 2027 (Planned) : PHS DSL Chemistry Builtins : Analytical ODE/PDE Solvers (dsolve/pdesolve) : Parametric Free-Body Diagrams (physure.diagrams) : GUM Welch-Satterthwaite & Real Gas EOS
    Q2 2027 (Planned) : Advanced Reaction Networks & Equilibrium : Cross-Language Chemistry Bindings : Verified Interval Arithmetic (IEEE 1788) : DPLL SAT Solver & Noether Symmetries
```

with:

```
    Q3 2026 (Completed) : Rust Core Engine : Correlated Uncertainty & Covariance : Chemistry Python Extension (Species & Reactions) : Native Symbolic AST & Regression
    Q4 2026 (In Progress) : Multivariate Polynomials & Gröbner Bases (F4/F5) : Partial Fraction Decomposition : Rust Core Chemistry Module (physure_core::chemistry) : Risch Integration Engine : Buckingham Pi Nullspace Solver
    Q1 2027 (Planned) : PHS DSL Chemistry Builtins : Analytical ODE/PDE Solvers (dsolve/pdesolve) : Bareiss Symbolic Matrices & Forms : Parametric Free-Body Diagrams (physure.diagrams) : GUM Welch-Satterthwaite & Real Gas EOS
    Q2 2027 (Planned) : Advanced Reaction Networks & Equilibrium : Cross-Language Chemistry Bindings : DAE Pantelides Index Reduction : Anisotropic 3D Tensors : Verified Interval Arithmetic (IEEE 1788) : DPLL SAT Solver & Noether Symmetries
```

- [ ] **Step 3: Write the same merged timeline into `docs/ROADMAP.md`**

In `docs/ROADMAP.md`, replace these four lines:

```
    Q3 2026 (Completed) : Rust Core Engine : Correlated Uncertainty & Covariance : Chemistry Extension (Species & Reactions) : Native Symbolic AST & Regression
    Q4 2026 (In Progress) : Multivariate Polynomials & Gröbner Bases (F4/F5) : Partial Fraction Decomposition : Risch Integration Engine : Buckingham Pi Nullspace Solver
    Q1 2027 (Planned) : Analytical ODE/PDE Solvers (dsolve/pdesolve) : Bareiss Symbolic Matrices & Forms : Parametric Free-Body Diagrams (physure.diagrams) : GUM Welch-Satterthwaite & Real Gas EOS
    Q2 2027 (Planned) : DAE Pantelides Index Reduction : Anisotropic 3D Tensors : Verified Interval Arithmetic (IEEE 1788) : DPLL SAT Solver & Noether Symmetries
```

with the same four merged lines as in Step 2.

- [ ] **Step 4: Verify the timelines are identical and nothing was lost**

```bash
diff <(grep -E "^ +Q[1-4] 20" ROADMAP.md) <(grep -E "^ +Q[1-4] 20" docs/ROADMAP.md); echo "timeline-diff-exit=$?"
for item in "Chemistry Python Extension" "Rust Core Chemistry Module" "PHS DSL Chemistry Builtins" "Advanced Reaction Networks" "Cross-Language Chemistry Bindings" "Partial Fraction Decomposition" "Bareiss Symbolic Matrices" "DAE Pantelides Index Reduction" "Anisotropic 3D Tensors"; do printf "%-34s root=%s docs=%s\n" "$item" "$(grep -c "$item" ROADMAP.md)" "$(grep -c "$item" docs/ROADMAP.md)"; done
git diff --stat ROADMAP.md docs/ROADMAP.md
```

Expected: `timeline-diff-exit=0`; every item prints `root=1 docs=1` except `DAE Pantelides Index Reduction`, which prints `root=2 docs=2` (each file also has a checklist line with that name); the stat shows only the four timeline lines changed in each file (4 insertions and 4 deletions each).

- [ ] **Step 5: Commit**

```bash
git add ROADMAP.md docs/ROADMAP.md
git diff --cached --name-only
git commit -m "docs(roadmap): merge the two timelines into one, keep both files" -m "ROADMAP.md and docs/ROADMAP.md had drifted: each timeline had items the other lacked. Both now carry the union. Each file keeps its own link base and everything else." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Commit the spec and the three plans

**Files:**
- Add: `docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md` (already committed once; now amended with verified facts)
- Add: `docs/superpowers/plans/2026-10-27-repo-hygiene-pr1-docs-truth.md`
- Add: `docs/superpowers/plans/2026-10-27-repo-hygiene-pr2-remove-noise.md`
- Add: `docs/superpowers/plans/2026-10-27-repo-hygiene-pr3-single-source-conf.md`

If `git log --oneline origin/main..HEAD` already shows a `docs(plans): ...` commit, this task was done while the plans were written: run Step 1 only and skip Step 2.

- [ ] **Step 1: Check the docs contain no markdown links to repo files**

```bash
grep -nE "\]\((\.\./|\./)?[A-Za-z_./-]+\.(rs|py|toml|md|yml|conf)\)" docs/superpowers/plans/2026-10-27-repo-hygiene-pr*.md docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md; echo "exit=$?"
```

Expected: no matches, `exit=1`. Any match is a real relative link: remove it or turn it into a GitHub blob URL.

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md docs/superpowers/plans/2026-10-27-repo-hygiene-pr1-docs-truth.md docs/superpowers/plans/2026-10-27-repo-hygiene-pr2-remove-noise.md docs/superpowers/plans/2026-10-27-repo-hygiene-pr3-single-source-conf.md
git diff --cached --name-only
git commit -m "docs(plans): repo hygiene implementation plans for PR-1, PR-2 and PR-3" -m "Also records in the spec what planning verified: the % interpolation root cause for the missing percent unit, the stale Cargo.lock, and the links that exclude_docs would break." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Verify the whole PR in a fresh environment

**Files:** none (verification only)

- [ ] **Step 1: Build this worktree's own environment**

```bash
cd physure-python
echo "VIRTUAL_ENV=${VIRTUAL_ENV:-<unset>}"   # on the maintainer machine this points at the main checkout's venv
unset VIRTUAL_ENV
uv sync --all-extras --dev
uv run maturin develop --release
uv run python -c "import physure._core as c; print(c.__file__)"
```

Expected: the printed path contains `.claude\worktrees\repo-hygiene\physure-python`. If it points at any other checkout, **stop**: the build went into a foreign venv; fix `VIRTUAL_ENV` and rerun. This takes several minutes.

- [ ] **Step 2: Run the documented test and lint commands**

```bash
uv run pytest tests/core/test_rust_parity.py -q
uv run ruff check . && uv run ruff format --check .
uv run pytest --collect-only -q | tail -2
cd ..
```

Expected: parity tests pass; ruff reports no problems; collection prints a test count with no errors. The last command is also the Makefile `test` recipe's first line (Task 4 Step 3).

- [ ] **Step 3: Strict docs build, as CI runs it**

```bash
cd physure-python
uv run mkdocs build --strict
cd ..
```

Expected: ends with `Documentation built in ... seconds` and no `WARNING` or `ERROR` lines. If it fails on something this PR did not touch, run the same command on a clean `origin/main` worktree to confirm it is pre-existing, then report instead of fixing.

- [ ] **Step 4: Confirm the diff contains only intended files**

```bash
git diff --name-only origin/main...HEAD
```

Expected exactly: `CLAUDE.md`, `CONTRIBUTING.md`, `Makefile`, `ROADMAP.md`, `docs/ROADMAP.md`, `structure.md`, the spec, and the three plans. `Cargo.lock` must **not** appear.

---

### Task 8: Push and open the PR (outward-facing: confirm with the maintainer first)

- [ ] **Step 1: Ask the maintainer to confirm pushing `docs/repo-hygiene-onboarding`**

The pre-push hook runs a full workspace build and test and is slow. Never use `--no-verify`.

- [ ] **Step 2: Push in the background with a long timeout**

```bash
git push -u origin docs/repo-hygiene-onboarding
```

Run it as a background command with the maximum timeout and wait for the notification.

- [ ] **Step 3: Open the PR**

```bash
gh pr create --base main --title "docs: make CLAUDE.md, CONTRIBUTING, structure.md and the Makefile match the code" --body-file - <<'EOF'
## What
- `CLAUDE.md`: Python commands run from `physure-python/`; maturin is invoked there too.
- `CONTRIBUTING.md`: seven crates (adds `physure-wasm`), docs preview, worktree venv warning.
- `structure.md`: real parser layout, mandatory native extension, Python-side duplication named.
- `Makefile`: targets run from `physure-python/` (they could not work from the root).
- The two roadmap timelines are merged (both files keep their content).
- Adds the repo-hygiene spec and the three implementation plans.

## Why
A new contributor following the docs hit commands that cannot run and claims the code contradicts.

## Verification
`uv run mkdocs build --strict`, parity tests, ruff, and the documented commands run in a fresh worktree.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

Expected: prints the PR URL. Report it to the maintainer.
