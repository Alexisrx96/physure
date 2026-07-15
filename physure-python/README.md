# physure 🔬

<div align="center">

<h3><b>High-Performance Physical Dimension Engine</b></h3>
<p><i>Rust-first. Zero-copy FFI. Successor to <code>physure</code>.</i></p>

[![PyPI](https://img.shields.io/pypi/v/physure)](https://pypi.org/project/physure/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Python](https://img.shields.io/pypi/pyversions/physure)](https://pypi.org/project/physure/)

</div>

---

## What is physure?

**physure** *(physics + measure)* is a unit-aware, dimension-correct engine for physical quantities — built on a pure Rust core with zero-copy FFI bridges to Python.

Born as the successor to `physure`, physure drops the pure-Python fallback in favor of a **Rust-first** architecture: the compiled extension is the only backend.

### Architecture

```
physure/                     # Cargo Workspace root
├── physure-core/            # 🦀 Pure Rust physics engine (no FFI deps)
└── physure-python/          # 🐍 PyO3 thin wrapper (zero-copy Buffer Protocol)
```

The core principle: **physure-core is the single source of truth**. All math lives in Rust. Python, and eventually WASM/Java, are thin translation layers.

---

## Installation

```bash
pip install physure           # Rust-compiled wheel
pip install "physure[numpy]"  # + NumPy/SciPy/Numba
pip install "physure[torch]"  # + PyTorch backend
pip install "physure[all]"    # All backends
```

### Building from Source

```bash
git clone https://github.com/Alexisrx96/physure
cd physure

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the Rust extension and install in dev mode
maturin develop --release

# Install Python dev dependencies
uv sync --group dev
```

---

## Quick Start

```python
from physure import Q_

# Classic physure syntax
d = Q_(10, "km")
t = Q_(2, "hr")
v = d / t
print(v.to("m/s"))   # 1.3888... m/s

# With uncertainty propagation
from physure import Quantity, units, create_default_system

sys = create_default_system()
m = Quantity(9.8, sys.get_unit("m/s^2"), uncertainty=0.02)
print(m)   # Quantity(9.8 ± 0.02 m/s²)
```

### Zero-Copy Batch Operations (New in v0.2.0)

```python
import numpy as np
from physure import batch_to_si_inplace, step_euler_inplace

# Convert 1M values to SI — Rust writes directly to NumPy memory
positions = np.random.rand(1_000_000).astype(np.float64)
batch_to_si_inplace(positions, factor=1000.0)  # km → m, in-place, zero copies

# Physics step: Euler integration
vel = np.ones(1_000_000, dtype=np.float64)
step_euler_inplace(positions, vel, dt=0.016)   # 60 FPS step
```

---

## Migrating from physure

`physure` is a **drop-in replacement**. The API is identical.

```bash
pip uninstall physure
pip install physure
```

```python
# Before
import physure
from physure import Q_

# After — one line change
import physure
from physure import Q_
```

See the full [Migration Guide](MIGRATION.md).

---

## Features

| Feature | Description |
|---------|-------------|
| **Rust Core** | All physics math in `physure-core`. Zero PyO3 in the engine. |
| **Zero-Copy FFI** | Buffer Protocol (`bytearray`, `memoryview`, `ndarray`) — no data copies |
| **Uncertainty Propagation** | Gaussian, Monte Carlo, and Unscented Transform backends |
| **Multi-Backend** | NumPy, PyTorch, JAX — `Quantity` wraps any array type |
| **JIT Compilation** | `torch.compile` / `jax.jit` — units evaporate at compile time |
| **Grammar Interpreter** | Physics-as-text DSL: `stress = 500 N / 2 m^2` |
| **REPL** | `python -m physure` — interactive unit-aware calculator |
| **Symbolic Math** | SymPy/SymEngine integration via `physure.ext.symbolic` |

---

## Workspace Structure

```
physure/
├── Cargo.toml               # Workspace orchestrator
├── pyproject.toml           # Python package (maturin)
│
├── physure-core/            # 🦀 Pure physics engine
│   ├── Cargo.toml           # No PyO3/WASM/JNI — hard rule
│   └── src/
│       ├── lib.rs           # Public API: RationalUnit, Quantity, etc.
│       ├── units.rs         # Dimensional analysis (rational exponents)
│       ├── uncertainty.rs   # Gaussian/MC/Unscented backends
│       ├── quantity.rs      # Physical quantity with propagation
│       ├── covariance.rs    # Sparse covariance store
│       ├── math.rs          # Numerical utilities
│       ├── serialization.rs # Arrow IPC serialization
│       └── symbolic.rs      # Symbolic expression engine
│
├── physure-python/          # 🐍 PyO3 thin wrapper
│   ├── Cargo.toml           # Depends on physure-core
│   ├── src/lib.rs           # All #[pyclass] / #[pymethods] live here
│   └── python/physure/      # Python shim
│       └── __init__.py      # Hard import of _core (no fallback)
│
└── physure/                 # Python package sources (application layer)
    ├── domain/              # Quantity, units, uncertainty Python layer
    ├── application/         # Factories, startup, context
    ├── backends/            # NumPy, PyTorch, JAX adapters
    ├── ext/                 # Grammar, IO, pandas, numba, chemistry
    └── nn/                  # Neural network integration
```

---

## License

**MIT License** — Irvin Torres
