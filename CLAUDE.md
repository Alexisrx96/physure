# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Install deps (including dev)
uv sync --all-extras

# Build Rust core (required after any change to physure-core/src/)
cd physure-core && maturin develop && cd ..

# Run all tests
uv run pytest

# Run a single test
uv run pytest tests/path/to/test_file.py::test_name -xvs

# Lint / format
uv run ruff check .
uv run ruff format .

# Type check
uv run ty check

# Enable runtime beartype contracts (slow, for debugging)
PHYSURE_DEBUG=1 uv run pytest

# SonarQube scan (requires .env with SONAR_TOKEN, see .env.example)
make sonar  # runs pytest coverage, Rust lcov, and pysonar analysis
```

`.env` holds `SONAR_TOKEN` and is gitignored. Copy `.env.example` to `.env` to configure.

`pytest` runs `--doctest-modules` by default, so doctests in source files are always executed.

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

## Philosophy & Correctness

These are the project's non-negotiable invariants, learned the hard way. Violating one is a bug even if all tests pass.

- **Unit correctness is the product.** Never silently drop a dimension, a conversion factor, or an uncertainty. If an operation can't preserve them, raise — a wrong answer with confident units is worse than an exception.
- **The Rust core comes first.** If Rust provides an implementation, Python MUST delegate to it directly; if no 3rd-party dependencies are involved, Rust delegation is mandatory.
- **Zero runtime dependencies is policy.** `dependencies = []` in pyproject.toml stays empty. Anything new goes in an optional extra (`[numpy]`, `[torch]`, `[io]`, ...) with a lazy import. The Rust core is not an extra — it ships compiled inside every wheel.
- **First use must stay fast (~0.5s budget).** `import physure` and the first `Q_()` evaluation must not pull in torch, scipy, or build more than one `UnitSystem`. Check with `time python -m physure "500 N / 2 m^2 => kPa"` after touching import paths (see PR #18 for the history).
- **Unit aliases collide silently.** `UnitSystem` logs a warning and the *later* definition wins (the `gal` gallon/galileo incident, PR #17). Before adding any unit or alias, grep the existing symbol across all `.conf` files — use the `add-unit` skill.
- **Global state must be resettable.** `CompoundUnit._cache`, `Dimension._cache`, and the global `CovarianceStore` are cleared by the `clean_state` autouse fixture. New global caches must be added to that fixture.
- **Doctests are tests.** pytest runs `--doctest-modules`; every docstring example in `physure/` executes on every run. Keep examples runnable or don't write them.
- **Never commit machine-specific config.** `.env`, `physure-core/.cargo/config.toml` with local rustflags, and `.claude/settings.local.json` broke or nearly broke CI before — they are gitignored; keep it that way.

### Code quality policy (enforced)

Nothing merges to main unless all of these hold. CI enforces the first three; the Sonar gate is checked locally with `make sonar`.

1. **Ruff clean**: `uv run ruff check .` and `uv run ruff format --check .` pass (CI `quality` job).
2. **Tests green with coverage ≥ 80%** total (`fail_under = 80` in pyproject.toml; CI runs pytest with `--cov`). New code should be born tested — if a module drops below the bar, add tests in the same PR, don't lower the bar.
3. **All four Python versions** (3.11–3.14) pass.
4. **SonarQube quality gate green** on new code, tiered by blast radius:
   - **`physure-core` and `physure-script`** (custom gate "Physure Core Strict"): new coverage ≥ 90%, new duplication ≤ 2%, zero new violations, security hotspots 100% reviewed. These are the no-fallback backbone — per the Philosophy section, "the Rust core comes first" and PHS ("only Rust implements it") has no Python fallback, so every other language binding (Python, WASM, CLI, LSP, Java) transitively depends on their correctness. A defect here has the widest blast radius in the repo, so it's held to a stricter bar.
   - **All other subprojects** (physure-python, physure-wasm, physure-cli, physure-lsp, physure-java) — baseline "Sonar way" gate: new coverage ≥ 80%, new duplication ≤ 3%, zero new violations, security hotspots 100% reviewed. These are thinner binding/delegation layers with lower intrinsic logic risk, and physure-python already has its own stricter 80%-coverage floor enforced separately in CI (see #2 above).
5. **ty is advisory, not gated** (~900 pre-existing errors). Don't add new errors to files you touch; burn down the backlog opportunistically.

### Changelog update policy (before every release)

Before tagging any release, `CHANGELOG.md` **must** be updated from the real git history. Never guess or assume which features belong to which release.

```bash
# 1. Find the last published tag
git tag -l --sort=-v:refname | head -5

# 2. List the real commits that will enter this release
git log --oneline --reverse <last-tag>..HEAD

# 3. Move the [Unreleased] section to the new version with today's date and tags
#    Example:  ## [0.2.4] - 2026-07-28
#              **Tags:** `v0.2.4`, `core-v0.2.4`

# 4. Create a fresh empty [Unreleased] section at the top
```

**Rules:**
- Every entry must correspond to a real commit between the previous tag and the current one.
- Do NOT attribute features to a release unless the commits prove they were included.
- Use commit messages as the source of truth for Added / Changed / Fixed / Removed.
- Follow [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format.
- The file lives at repository root: `CHANGELOG.md`.

