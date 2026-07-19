<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-dark.svg">
  <img src="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-light.svg" alt="physure" width="440">
</picture>

<h3><b>Unit-aware, dimension-correct computing for Python — powered by a Rust core</b></h3>
<p><i>Units, dimensions, and correlated uncertainty tracked through every calculation, with zero overhead under <code>torch.compile</code> / <code>jax.jit</code>.</i></p>

[![PyPI](https://img.shields.io/pypi/v/physure)](https://pypi.org/project/physure/)
[![Python 3.11–3.14](https://img.shields.io/badge/python-3.11%20%7C%203.12%20%7C%203.13%20%7C%203.14-blue)](https://pypi.org/project/physure/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/Alexisrx96/physure/blob/main/LICENSE)
[![Rust core](https://img.shields.io/badge/core-Rust%20%F0%9F%A6%80-orange)](https://github.com/Alexisrx96/physure/tree/main/physure-core/)

</div>

---

## Why physure?

Most unit libraries make you choose between correctness and speed. **physure** *(physics + measure)* refuses the trade-off:

- **Correlated uncertainty propagation.** Full sparse-covariance tracking between quantities (GUM-style), not just independent error bars. If `x` and `y` share history, `x - y` knows it.
- **Native-speed dimensional analysis.** Unit arithmetic runs in Rust (~50 ns per operation) with rational exponents — no floating-point drift in dimensions, zero-copy buffer FFI to NumPy.
- **ML-ready.** `Quantity` wraps NumPy, PyTorch, and JAX arrays. Under `@physure.jit`, units are validated at trace time and *evaporate* at runtime: ~1.17× vs. raw compiled PyTorch.
- **Static unit checking.** A mypy plugin narrows `Q_(3, "m/s")` to `Quantity[..., Literal["m/s"]]`, so unit mismatches can fail before your code even runs.
- **Zero runtime dependencies.** `pip install physure` pulls in nothing else. NumPy/PyTorch/JAX/pandas support activates automatically when those packages are present.

## Quick start

```python
from physure import Q_

d = Q_(10, "km")
t = Q_(2, "hr")
print((d / t).to("m/s"))    # 1.3888888888888888 m/s

# Uncertainty propagates automatically — correlations included
g = Q_(9.8, "m/s^2", uncertainty=0.02)
m = Q_(2.5, "kg", uncertainty=0.001)
E = m * g * Q_(12, "m")
print(E.to("J"))            # (294.0 ± 0.61) J
```

Or straight from the command line — physure ships a unit-aware calculator and REPL:

```bash
$ python -m physure "500 N / 2 m^2 => kPa"
0.25 kPa
```

## Highlights

### Units that vanish at compile time

`@physure.jit` traces your function once, validates every dimension in Rust, then runs on raw tensors — dimensional safety with no per-call cost:

```python
from physure import Q_, jit

@jit
def kinetic_energy(mass, velocity):
    return 0.5 * mass * velocity**2

kinetic_energy(Q_(10.0, "kg"), Q_(5.0, "m/s"))   # 125.0 kg·m²/s²
kinetic_energy(Q_(1.0, "m"), Q_(1.0, "s"))       # raises at trace time: incompatible units
```

Works with plain floats, NumPy arrays, PyTorch tensors (via `__torch_dispatch__` + `torch.compile`), and JAX (`jax.jit`).

### Uncertainty done properly

Choose the propagation mode globally or per-block:

```python
import physure

physure.propagation_mode("correlated")     # full covariance (default: uncorrelated)

with physure.uncertainty_mode("uncorrelated"):
    ...                                    # scoped override
```

Backends include Gaussian (first-order), Monte Carlo, and Unscented Transform. Covariance lives in a sparse Rust store, so large lineages stay fast and memory stays flat.

### Batteries included, loaded lazily

Pandas ExtensionArray, pydantic validation, SymPy symbolic quantities, unit-aware `torch.nn` layers, Arrow IPC serialization, plotting helpers, and a physics-as-text DSL (`stress = 500 N / 2 m^2`) — each activates only when you use it. Cold import stays around **20 ms**.

## How it compares

| | physure | pint | astropy.units | unyt |
|---|:---:|:---:|:---:|:---:|
| Correlated uncertainty (covariance) | ✅ | — | — | — |
| Built-in uncertainty propagation | ✅ | via `uncertainties` | limited | — |
| Rust-accelerated core | ✅ | — | — | — |
| `torch.compile` / `jax.jit` compatible | ✅ | — | — | — |
| Static unit checking (mypy) | ✅ | — | — | — |
| Runtime dependencies | none | none | astropy stack | numpy |
| Ecosystem maturity | new | ✅ mature | ✅ mature | mature |

If you need a battle-tested converter with a decade of integrations, pint and astropy are excellent. physure is for when you also need **uncertainty you can defend** and **units inside compiled ML code**.

## Performance

| Benchmark | Result |
|---|---|
| Cold import | ~21 ms |
| Unit multiply/divide (Rust core) | ~54 ns |
| Scalar add with dimension check | ~40 ns |
| 10⁶-element tensor op, `@torch.compile` | 1.17× vs. pure PyTorch |
| Covariance propagation (sparse blocks) | ~7 µs |

Full methodology and reproduction steps: [BENCHMARKS.md](https://github.com/Alexisrx96/physure/blob/main/BENCHMARKS.md).

## Installation

```bash
pip install physure            # Rust-compiled wheel, no other dependencies
pip install "physure[numpy]"   # + NumPy/SciPy/Numba acceleration
pip install "physure[torch]"   # + PyTorch backend
pip install "physure[jax]"     # + JAX backend
pip install "physure[all]"     # everything
```

### From source

```bash
git clone https://github.com/Alexisrx96/physure
cd physure
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust, if needed
maturin develop --release
uv sync --group dev
```

## Architecture

```
physure/                     # Cargo workspace root
├── physure-core/            # 🦀 Pure Rust physics engine — no FFI deps
│   └── src/                 #    units, quantity, covariance, uncertainty,
│                            #    symbolic, Arrow serialization
└── physure-python/          # 🐍 PyO3 bindings + Python application layer
    └── physure/
        ├── domain/          # Quantity, units, dimensions, uncertainty
        ├── application/     # Q_ factory, unit-system context, startup
        ├── backends/        # NumPy / PyTorch / JAX adapters
        ├── _jit/            # tracing + compile-time dimension checks
        ├── ext/             # grammar DSL, IO, pandas, numba
        └── nn/              # unit-aware neural network layers
```

The rule: **physure-core is the single source of truth**. All math lives in Rust; Python is a thin, zero-copy translation layer.

## Documentation

- [Unit reference](https://github.com/Alexisrx96/physure/blob/main/physure-python/docs/UNITS.md) — every unit, prefix, and constant
- [Tutorials](https://github.com/Alexisrx96/physure/tree/main/physure-python/docs/tutorials/) and [examples](https://github.com/Alexisrx96/physure/tree/main/physure-python/examples/) — including a unit-checked [PINN notebook](https://github.com/Alexisrx96/physure/blob/main/physure-python/examples/pinn_harmonic_oscillator.ipynb)
- [torch.compile integration](https://github.com/Alexisrx96/physure/blob/main/physure-python/docs/torch_compile_integration.md)

## Contributing

Issues and PRs are welcome. The quality bar is enforced in CI: ruff clean, tests green on Python 3.11–3.14 with ≥ 80 % coverage, and zero new SonarQube violations. See [CLAUDE.md](https://github.com/Alexisrx96/physure/blob/main/CLAUDE.md) for the full development guide.

## License

[MIT](https://github.com/Alexisrx96/physure/blob/main/LICENSE) — Irvin Torres
