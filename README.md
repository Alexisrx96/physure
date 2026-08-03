<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-horizontal-dark.svg">
  <img src="assets/logo-horizontal-light.svg" alt="physure" width="440">
</picture>

<h3><b>Unit-aware, dimension-correct physics computing — from a shared Rust core</b></h3>
<p><i>The same dimensional analysis and uncertainty engine, exposed as a standalone DSL (<code>phs</code>), a Python library, a Rust crate, and a JVM package — with zero overhead under <code>torch.compile</code> / <code>jax.jit</code> where it matters.</i></p>

[![PyPI](https://img.shields.io/pypi/v/physure?color=F59E0B&labelColor=18181A)](https://pypi.org/project/physure/)
[![crates.io](https://img.shields.io/crates/v/physure?color=F59E0B&labelColor=18181A)](https://crates.io/crates/physure)
[![Maven Central](https://img.shields.io/maven-central/v/io.github.alexisrx96/physure-java?color=F59E0B&labelColor=18181A)](https://central.sonatype.com/artifact/io.github.alexisrx96/physure-java)
[![CI](https://img.shields.io/github/actions/workflow/status/Alexisrx96/physure/tests.yml?branch=main&labelColor=18181A)](https://github.com/Alexisrx96/physure/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-F59E0B?labelColor=18181A)](LICENSE)

</div>

---

## What's in this repo

physure is a Rust physics/measurement engine (`physure-core/`) with four ways to use it. Pick
the one that matches how you work — none of them require the others:

| | What it is | Needs Python? |
|---|---|:---:|
| **[PHS](physure-cli/README.md)** | A standalone DSL + CLI (`phs`) for writing engineering/lab calculations as physics, not code — validates a formula before it's handed to a dev team, and doubles as documentation of the calculation process. | No |
| **[physure (Python)](physure-python/)** | The library this README focuses on below: `Quantity`, NumPy/PyTorch/JAX backends, `@physure.jit`. | Yes (it *is* the Python package) |
| **[physure (Rust)](physure-core/README.md)** | The core crate directly, for Rust projects that want dimensional analysis and uncertainty propagation without any FFI. | No |
| **[physure-java](physure-java/README.md)** | JNI bindings for JVM projects, published to Maven Central. | No |

Full install instructions for all four: **[INSTALL.md](INSTALL.md)**.

The rest of this page covers the **Python library**.

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

The Python package also ships a small unit-aware calculator:

```bash
$ python -m physure "500 N / 2 m^2 => kPa"
0.25 kPa
```

Prefer to skip Python entirely and run calculations as standalone scripts? See **[PHS](physure-cli/README.md)** — a single native binary (`phs`), no interpreter or virtualenv required.

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

physure.propagation_mode("uncorrelated")   # independent errors (default: correlated)

with physure.uncertainty_mode("uncorrelated"):
    ...                                    # scoped override
```

Backends include Gaussian (first-order), Monte Carlo, and Unscented Transform. Covariance lives in a sparse Rust store, so large lineages stay fast and memory stays flat.

The default is correlated because the alternative is quietly wrong: with independent errors `x - x` reports `σ·√2` instead of zero, which looks plausible enough to publish. `[Settings] propagation_mode` in `physure.conf` sets it for a project, and both the core and PHS read it, so they cannot disagree.

### Batteries included, loaded lazily

Pandas ExtensionArray, pydantic validation, SymPy symbolic quantities, unit-aware `torch.nn` layers, and Arrow IPC serialization each activate only when you use them. Cold import stays around **20 ms**.

## How it compares

| | physure | pint | astropy.units | unyt |
|---|:---:|:---:|:---:|:---:|
| Correlated uncertainty (covariance) | ✅ | — | — | — |
| Built-in uncertainty propagation | ✅ | via `uncertainties` | limited | — |
| Rust-accelerated core | ✅ | — | — | — |
| `torch.compile` / `jax.jit` compatible | ✅ | — | — | — |
| Static unit checking (mypy) | ✅ | — | — | — |
| Standalone CLI/DSL, no Python required | ✅ ([PHS](physure-cli/README.md)) | — | — | — |
| Runtime dependencies | none | none | astropy stack | numpy |
| Ecosystem maturity | new | ✅ mature | ✅ mature | mature |

If you need a battle-tested converter with a decade of integrations, pint and astropy are excellent. physure is for when you also need **uncertainty you can defend**, **units inside compiled ML code**, or a way to hand a validated calculation to a dev team without a Python file changing hands.

## Performance

| Benchmark | Result |
|---|---|
| Cold import | ~21 ms |
| Unit multiply/divide (Rust core) | ~54 ns |
| Scalar add with dimension check | ~40 ns |
| 10⁶-element tensor op, `@torch.compile` | 1.17× vs. pure PyTorch |
| Covariance propagation (sparse blocks) | ~7 µs |

Full methodology and reproduction steps: [BENCHMARKS.md](BENCHMARKS.md).

## Installation

```bash
pip install physure            # Rust-compiled wheel, no other dependencies
pip install "physure[numpy]"   # + NumPy/SciPy/Numba acceleration
pip install "physure[torch]"   # + PyTorch backend
pip install "physure[jax]"     # + JAX backend
pip install "physure[all]"     # everything
```

Installing the `phs` CLI, the Rust crate, the Java bindings, or the VS Code extension instead? See **[INSTALL.md](INSTALL.md)**.

### From source (Python package)

```bash
git clone https://github.com/Alexisrx96/physure
cd physure
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust, if needed
cd physure-python
maturin develop --release
uv sync --group dev
```

## Architecture

```
physure/                     # Cargo workspace root
├── physure-core/            # 🦀 Pure Rust physics engine — no FFI deps
│   └── src/                 #    units, quantity, covariance, uncertainty,
│                            #    symbolic, Arrow serialization
├── physure-python/          # 🐍 PyO3 bindings + Python application layer
│   └── physure/
│       ├── domain/          # Quantity, units, dimensions, uncertainty
│       ├── application/     # Q_ factory, unit-system context, startup
│       ├── backends/        # NumPy / PyTorch / JAX adapters
│       ├── _jit/            # tracing + compile-time dimension checks
│       ├── ext/             # IO, pandas, numba, chemistry, symbolic regression,
│       │                    #   AOT compiler, PHS plugin loader
│       └── nn/              # unit-aware neural network layers
├── physure-script/          # PHS language engine — lexer, parser, interpreter, transpiler
├── physure-cli/             # the `phs` binary: REPL, TUI, web visualizer, HTML reports
├── physure-lsp/             # language server for PHS (consumed by the VS Code extension)
└── physure-java/            # JNI bindings, published to Maven Central
```

The rule: **physure-core is the single source of truth**. All dimensional-analysis and
uncertainty math lives in Rust; every other package — Python, PHS, Java — is a thin translation
layer on top of it.

## Documentation

- [PHS README](physure-cli/README.md) — the standalone DSL and CLI
- [Unit reference](physure-python/docs/UNITS.md) — every unit, prefix, and constant
- [Tutorials](physure-python/docs/tutorials/) and [examples](physure-python/examples/) — including a unit-checked [PINN notebook](physure-python/examples/pinn_harmonic_oscillator.ipynb)
- [torch.compile integration](physure-python/docs/torch_compile_integration.md)

## Contributing

Issues and PRs are welcome. The quality bar is enforced in CI: ruff clean, tests green on Python 3.11–3.14 with ≥ 80 % coverage, and zero new SonarQube violations. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build and test the workspace, the invariants a change has to hold, and the commit conventions.

## License

[MIT](LICENSE) — Irvin Torres
