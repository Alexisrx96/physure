# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Install deps (including dev)
uv sync --all-extras

# Build Rust core (required after any change to physure_core/src/)
cd physure_core && maturin develop && cd ..

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

# SonarQube scan (requires .env with SONAR_TOKEN, server at http://localhost:9000)
uv run pytest tests/ --cov=physure --cov-report=xml --junitxml=test-results.xml
make sonar  # runs pytest+coverage then pysonar
```

`.env` holds `SONAR_TOKEN` and is gitignored. Never commit it.

`pytest` runs `--doctest-modules` by default, so doctests in source files are always executed.

## Architecture

### Two packages, one repo

- **`physure/`** — pure Python library (the public API)
- **`physure_core/`** — Rust extension via PyO3, built with `maturin`. Provides `RationalUnit`, `UnitRegistry`, `CovarianceStore`, `PruningConfig`. All imports from `physure_core` have Python fallbacks so the library still works without it, at reduced performance.

### Layer map

```
application/    ← factories (Q_), context (ContextVar), startup (parse .conf → UnitSystem), IO
domain/         ← core business logic
  measurement/  ← Quantity, Dimension, CompoundUnit, UnitSystem, Uncertainty, converters
  notation/     ← lexer, parsers, AST for unit expressions
  symbolic/     ← sympy-based symbolic quantity handling
core/           ← BackendManager dispatcher, protocols, formatting
backends/       ← per-backend ops (numpy_backend, torch_backend, jax_backend)
  torch/        ← autograd store, covariance for PyTorch
_jit/           ← tracing + kernel baking; uses RationalUnit for compile-time dim checks
infrastructure/ ← .conf files defining SI and Imperial unit systems
ext/            ← optional pandas/numba integrations (activated lazily in __init__.py)
nn/             ← unit-aware neural network layers (wraps torch.nn)
static/         ← custom mypy plugin, generated type stubs
```

### Key objects and how they relate

**`Quantity[Value, Unc, Unit]`** (`domain/measurement/quantity.py`) is the central user-facing class. It holds a `magnitude` (any numeric type), a `CompoundUnit`, and an optional `Uncertainty`.

**`CompoundUnit`** and **`Dimension`** both use the Flyweight pattern — they cache instances in class-level `_cache` dicts. Call `CompoundUnit._cache.clear()` and `Dimension._cache.clear()` when resetting state (the test conftest does this automatically).

**`UnitSystem`** (`domain/measurement/system.py`) is a self-contained registry of dimensions, units, prefixes, and constants. The active system is stored in a `ContextVar` (`application/context.py`). Resolution order: ContextVar → global cache → lazy-load default SI system from `.conf` files.

**`Q_`** is a `QuantityFactory` instance (not a class), called like `Q_(10, "m/s")`. It reads the active `UnitSystem` from context.

**`Uncertainty`** is abstract (`domain/measurement/uncertainty.py`). Concrete subclasses: `CorrelatedUncertainty` (tracks full covariance via `CovarianceStore`) and `UncorrelatedUncertainty`. The global `CovarianceStore` must be cleared between tests — handled by the `clean_state` autouse fixture in `tests/conftest.py`.

**`BackendManager`** (`core/dispatcher.py`) lazily loads backend modules (NumPy, PyTorch, JAX) and caches them. Select via `PHYSURE_BACKEND` env var or by passing a tensor to `Q_`.

### JIT compilation

`physure.jit` wraps a function with symbolic tracing: during the trace, `Quantity` objects are unwrapped and unit arithmetic is validated via `RationalUnit` (Rust). The compiled function operates on raw tensors; unit checks have zero runtime overhead. For PyTorch, this goes through `__torch_dispatch__`.

### Unit system bootstrap

`application/startup.py` reads `infrastructure/config/physure.conf`, `systems/international.conf`, and `systems/imperial.conf` using `configparser`, then uses `UnitSystemBuilder` to register dimensions, prefixes, units, and constants into a `UnitSystem`. A user-local `physure.conf` in the CWD can override defaults.

### Uncertainty propagation modes

Set globally with `physure.propagation_mode("correlated" | "uncorrelated")` or scoped with the `uncertainty_mode` context manager. Correlated mode uses a sparse covariance matrix (backed by `physure_core.CovarianceStore` when available). For JAX/functional use, `physure.functional.FunctionalState` lets you pass the covariance matrix explicitly instead of relying on global state.

### mypy plugin

`physure.static.mypy_plugin` narrows `Q_("value", "unit_str")` return types to `Quantity[..., ..., Literal["unit_str"]]` at static analysis time. Configured in both `pyproject.toml` and `mypy.ini`.

## Philosophy & Correctness

These are the project's non-negotiable invariants, learned the hard way. Violating one is a bug even if all tests pass.

- **Unit correctness is the product.** Never silently drop a dimension, a conversion factor, or an uncertainty. If an operation can't preserve them, raise — a wrong answer with confident units is worse than an exception.
- **The Rust core is always optional.** Every `from physure_core import ...` must have a working pure-Python fallback. New Rust features land with the fallback in the same PR.
- **Zero runtime dependencies is policy.** `dependencies = []` in pyproject.toml stays empty. Anything new goes in an optional extra (`[native]`, `[numpy]`, ...) with a lazy import.
- **First use must stay fast (~0.5s budget).** `import physure` and the first `Q_()` evaluation must not pull in torch, scipy, or build more than one `UnitSystem`. Check with `time python -m physure "500 N / 2 m^2 => kPa"` after touching import paths (see PR #18 for the history).
- **Unit aliases collide silently.** `UnitSystem` logs a warning and the *later* definition wins (the `gal` gallon/galileo incident, PR #17). Before adding any unit or alias, grep the existing symbol across all `.conf` files — use the `add-unit` skill.
- **Global state must be resettable.** `CompoundUnit._cache`, `Dimension._cache`, and the global `CovarianceStore` are cleared by the `clean_state` autouse fixture. New global caches must be added to that fixture.
- **Doctests are tests.** pytest runs `--doctest-modules`; every docstring example in `physure/` executes on every run. Keep examples runnable or don't write them.
- **Never commit machine-specific config.** `.env`, `physure_core/.cargo/config.toml` with local rustflags, and `.claude/settings.local.json` broke or nearly broke CI before — they are gitignored; keep it that way.

### Code quality policy (enforced)

Nothing merges to main unless all of these hold. CI enforces the first three; the Sonar gate is checked locally with `make sonar`.

1. **Ruff clean**: `uv run ruff check .` and `uv run ruff format --check .` pass (CI `quality` job).
2. **Tests green with coverage ≥ 80%** total (`fail_under = 80` in pyproject.toml; CI runs pytest with `--cov`). New code should be born tested — if a module drops below the bar, add tests in the same PR, don't lower the bar.
3. **All five Python versions** (3.10–3.14) pass.
4. **SonarQube quality gate green** on new code: coverage ≥ 80%, duplication ≤ 3%, zero new violations.
5. **ty is advisory, not gated** (~900 pre-existing errors). Don't add new errors to files you touch; burn down the backlog opportunistically.
