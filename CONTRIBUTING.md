# Contributing to Physure

Thanks for being here. Physure is a physical-quantity engine: units, dimensions and
uncertainty, with a Rust core and Python, Java and PHS front ends. This file is what you
need to build it, test it, and get a change merged.

Questions, proposals and "is this in scope?" are welcome as issues before any code. A
design discussion costs one thread; a rejected PR costs an afternoon.

---

## Layout

One repository, six crates and one Python package. The names do not all match their
directories, which trips people up:

| Directory | What it is | Published as |
| --- | --- | --- |
| `physure-core/` | The engine: units, dimensions, uncertainty, propagation | crate **`physure`** on crates.io — never `physure-core` |
| `physure-script/` | PhysureScript: pest grammar, interpreter, transpilers | crate `physure-script` |
| `physure-cli/` | The `phs` binary | crate `physure-cli` |
| `physure-python/` | The Python package and the PyO3 bindings | `physure` on PyPI |
| `physure-java/` | The Java classes and the JNI bridge | Maven |
| `physure-lsp/` | The language server for PHS | crate `physure-lsp` |

The Rust library target inside `physure-core/` is called `physure_core`, so Rust code says
`use physure_core::...` while `Cargo.toml` says `physure`. That split is deliberate; please
do not "fix" it.

Inside `physure-python/physure/`:

```
application/    factories (Q_), context (ContextVar), startup (.conf -> UnitSystem), IO
domain/         Quantity, Dimension, CompoundUnit, UnitSystem, Uncertainty, notation, symbolic
core/           BackendManager dispatcher, protocols, formatting
backends/       per-backend ops (numpy, torch, jax)
_jit/           tracing and kernel baking
infrastructure/ the .conf files defining SI and Imperial
ext/, nn/       optional integrations, unit-aware torch layers
```

---

## Build

You need Rust (stable), [uv](https://docs.astral.sh/uv/), and a JDK only if you touch Java.

**The Python virtualenv lives in `physure-python/`, not at the repository root.** Every `uv`
command below is run from there.

```bash
# Rust only
cargo build
cargo build --release --bin phs          # the phs CLI -> target/release/phs

# Python, including the native extension
cd physure-python
uv sync --all-extras --dev
uv run maturin develop --release         # rebuild after ANY change under physure-core/src
```

`maturin develop` is not optional after touching Rust: the Python package imports a compiled
`physure._core`, and a stale one will have you chasing a bug you already fixed. The same goes
for `target/release/phs` — rebuild it before trusting what the CLI prints.

---

## Test

```bash
cargo test -p physure -p physure-script -p physure-cli -p physure-lsp   # what CI runs
cargo test --workspace                                                  # adds the binding crates

cd physure-python
uv run pytest                            # the whole suite, a few minutes
uv run pytest tests/test_uncertainty.py::test_name -xvs   # one test
```

`pytest` runs with `--doctest-modules`, so **every docstring example under `physure/` is a
test**. Keep examples runnable or leave them out.

`tests/conftest.py` has an autouse `clean_state` fixture that clears `CompoundUnit._cache`,
`Dimension._cache` and the global `CovarianceStore` between tests. Any new global cache has
to be added there, or you will spend a day on a test that only fails when run after another.

One known flake: `tests/test_performance_and_interop.py::test_arrow_speed` asserts against a
wall clock and sits close to its threshold. A lone failure there is a busy machine.

---

## Quality gates

CI enforces the first three; the fourth is checked with `make sonar` and a `.env` holding
`SONAR_TOKEN` (copy `.env.example`).

1. **Ruff clean** — `uv run ruff check .` and `uv run ruff format --check .`
2. **Tests green, coverage ≥ 80%** — new code should arrive tested; if a module drops below
   the bar, add tests in the same PR rather than lowering it
3. **Python 3.11 through 3.14** all pass
4. **SonarQube gate green on new code** — coverage ≥ 80%, duplication ≤ 3%, no new violations
5. **`ty` is advisory** — there is a backlog of pre-existing errors. Don't add new ones to
   files you touch; burn the old ones down when you happen to be there.

---

## The invariants

These are not style preferences. Breaking one is a bug even when every test passes.

- **Unit correctness is the product.** Never silently drop a dimension, a conversion factor
  or an uncertainty. If an operation cannot preserve them, raise. A wrong answer with
  confident units is worse than an exception.
- **The Rust core comes first.** If the core has an implementation, the other languages
  delegate to it. Two implementations of the same rule become two answers.
- **Zero runtime dependencies.** `dependencies = []` in `physure-python/pyproject.toml` stays
  empty. Anything new goes in an optional extra with a lazy import.
- **First use stays fast (~0.5s).** `import physure` and the first `Q_()` must not pull in
  torch or scipy or build more than one `UnitSystem`. Check with
  `time python -m physure "500 N / 2 m^2 => kPa"` after touching import paths.
- **Unit aliases collide silently.** `UnitSystem` logs a warning and the *later* definition
  wins. Before adding any unit or alias, grep the symbol across every `.conf` file — this is
  how `gal` became galileo instead of gallon.
- **Global state must be resettable.** See the `clean_state` fixture above.
- **Never commit machine-specific config.** `.env`, a `physure_core/.cargo/config.toml` with
  local rustflags, and `.claude/settings.local.json` are gitignored. They have broken CI
  before.

---

## Commits and pull requests

- One logical change per commit. A refactor and a fix are two commits, even in one file.
- [Conventional commits](https://www.conventionalcommits.org/): `fix(phs): ...`,
  `feat(python): ...`, `docs: ...`. The scope is the component you touched.
- Write the subject as what the code now does, and the body as what was wrong before and why
  the new behaviour is right. The history is where the reasoning lives.
- Check `git diff --cached --name-only` before every commit. Sweeping an unrelated file in is
  the most common review comment here.
- PRs go to `main`. Keep them to one topic; a three-part feature is three PRs in sequence
  rather than one large one.

### CHANGELOG

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Add your
entry to `[Unreleased]` in the same PR, under Added / Changed / Fixed / Removed.

Entries are written from the real git history, never from memory:

```bash
git tag -l --sort=-v:refname | head -5
git log --oneline --reverse <last-tag>..HEAD
```

An entry says what changed and, when something was wrong, what it did instead — the reader
is deciding whether the release affects a result they have already published.

Releases and version bumps are the maintainer's call; please don't include them in a PR.

---

## Working on uncertainty

The area moves fastest and has the most invariants, so a few pointers.

`Uncertainty` is abstract on the Python side; the concrete models are `CorrelatedUncertainty`
(full covariance through `CovarianceStore`) and `UncorrelatedUncertainty`. In the Rust core
the models are the `UncertaintyValue` variants, and scalar provenance lives in
`uncertainty::lineage` — that is what makes `x - x` exactly zero rather than `σ√2`.

Two knobs, deliberately separate:

- `physure.propagation_mode("correlated" | "uncorrelated")` — how correlations are handled.
  Also readable from `[Settings]` in `physure.conf`.
- `physure.uncertainty_model("gaussian" | "moments")` — what shape the distribution has.

`physure_core::uncertainty::moments` carries asymmetric measurements as the first three
moments and converts to and from a quoted `(σ⁻, σ⁺)` pair. Nothing propagates the third
moment yet, so **every arithmetic path on a moments value raises**, and PHS refuses the
literal rather than reporting the measurement as symmetric. If you are adding propagation
there, keep that property: falling back to a symmetric rule produces a plausible number with
the asymmetry averaged away, which is the failure the model exists to prevent.
