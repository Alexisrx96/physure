# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Install deps (including dev)
uv sync --all-extras

# Build Rust core (required after any change to measurekit_core/src/)
cd measurekit_core && maturin develop && cd ..

# Run all tests
uv run pytest

# Run a single test
uv run pytest tests/path/to/test_file.py::test_name -xvs

# Lint / format
uv run ruff check .
uv run ruff format .

# Type check
uv run mypy measurekit

# Enable runtime beartype contracts (slow, for debugging)
MEASUREKIT_DEBUG=1 uv run pytest

# SonarQube scan (requires .env with SONAR_TOKEN, server at http://localhost:9000)
uv run pytest tests/ --cov=measurekit --cov-report=xml --junitxml=test-results.xml
make sonar  # runs pytest+coverage then pysonar
```

`.env` holds `SONAR_TOKEN` and is gitignored. Never commit it.

`pytest` runs `--doctest-modules` by default, so doctests in source files are always executed.

## Architecture

### Two packages, one repo

- **`measurekit/`** — pure Python library (the public API)
- **`measurekit_core/`** — Rust extension via PyO3, built with `maturin`. Provides `RationalUnit`, `UnitRegistry`, `CovarianceStore`, `PruningConfig`. All imports from `measurekit_core` have Python fallbacks so the library still works without it, at reduced performance.

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

**`BackendManager`** (`core/dispatcher.py`) lazily loads backend modules (NumPy, PyTorch, JAX) and caches them. Select via `MEASUREKIT_BACKEND` env var or by passing a tensor to `Q_`.

### JIT compilation

`measurekit.jit` wraps a function with symbolic tracing: during the trace, `Quantity` objects are unwrapped and unit arithmetic is validated via `RationalUnit` (Rust). The compiled function operates on raw tensors; unit checks have zero runtime overhead. For PyTorch, this goes through `__torch_dispatch__`.

### Unit system bootstrap

`application/startup.py` reads `infrastructure/config/measurekit.conf`, `systems/international.conf`, and `systems/imperial.conf` using `configparser`, then uses `UnitSystemBuilder` to register dimensions, prefixes, units, and constants into a `UnitSystem`. A user-local `measurekit.conf` in the CWD can override defaults.

### Uncertainty propagation modes

Set globally with `measurekit.propagation_mode("correlated" | "uncorrelated")` or scoped with the `uncertainty_mode` context manager. Correlated mode uses a sparse covariance matrix (backed by `measurekit_core.CovarianceStore` when available). For JAX/functional use, `measurekit.functional.FunctionalState` lets you pass the covariance matrix explicitly instead of relying on global state.

### mypy plugin

`measurekit.static.mypy_plugin` narrows `Q_("value", "unit_str")` return types to `Quantity[..., ..., Literal["unit_str"]]` at static analysis time. Configured in both `pyproject.toml` and `mypy.ini`.
