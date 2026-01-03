<div align="center">
  <img src="https://raw.githubusercontent.com/Alexisrx96/measurekit/main/MeasureKitLogoBeta.jpg" alt="MeasureKit Logo" width="500">
  <br>
  <h1>MeasureKit</h1>
  <p><b>A Zero-Overhead, Multi-Backend Physical Dimension Engine for Python</b></p>

  <p>
    <a href="https://pypi.org/project/measurekit/"><img src="https://img.shields.io/pypi/v/measurekit.svg?style=flat-square" alt="PyPI version"></a>
    <a href="https://github.com/Alexisrx96/measurekit/actions"><img src="https://img.shields.io/github/actions/workflow/status/Alexisrx96/measurekit/tests.yml?branch=main&style=flat-square" alt="Build Status"></a>
    <a href="https://codecov.io/gh/Alexisrx96/measurekit"><img src="https://img.shields.io/codecov/c/github/Alexisrx96/measurekit?style=flat-square" alt="Coverage"></a>
    <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="License: MIT"></a>
    <a href="https://astral.sh/uv"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json&style=flat-square" alt="uv"></a>
  </p>
</div>

---

## ⚡ Why MeasureKit?

MeasureKit is not just another unit library. It's a high-performance engine designed for modern scientific Python stack. It bridges the gap between scalar physical calculations and vectorized, multi-backend tensor operations.

- **🚀 Ultra-Fast:** Optimized `_fast_new` path for zero-overhead arithmetic.
- **🛡️ Type Safe:** Built-in Pydantic V2 support for robust data validation.
- **🔌 Backend Agnostic:** Transparently switch between **NumPy**, **PyTorch**, and **JAX**.
- **📈 Uncertainty Propagation:** Advanced correlated error tracking using the Affine Transformation Formula.
- **🧩 Symbolic Analysis:** Native SymPy integration for dimensional verification.
- **⚡ Zero-Overhead Lazy Loading:** Units are transpiled to Python bytecode at build time, ensuring sub-millisecond import costs.

---

## 📦 Installation

We recommend using [uv](https://astral.sh/uv) for lightning-fast installation:

```bash
# Basic installation
uv pip install measurekit

# With all backend dependencies (NumPy, Torch, JAX, Pandas)
uv pip install "measurekit[all]"
```

---

## 🔥 Quick Start

### 1. Simple Scalar Arithmetic

```python
from measurekit import Q_

dist = Q_(10, "km")
time = Q_(2, "hr")

speed = dist / time
print(speed)              # 5.0 km/hr
print(speed.to("m/s"))    # 1.3889 m/s
```

### 2. Multi-Backend Vectorization

MeasureKit automatically detects the backend (NumPy, Torch, or JAX) and uses the appropriate high-performance kernels.

```python
import torch
from measurekit import Q_

# Initialize with a PyTorch tensor
tensor = torch.tensor([1.0, 2.0, 3.0], device="cuda")
q = Q_(tensor, "m/s")

# All operations stay on GPU and use Torch kernels
res = q * Q_(2, "s")
print(res.magnitude)  # tensor([2., 4., 6.], device='cuda:0')
```

### 3. Pydantic Integration

Ensure your data models are physically sound with automatic validation.

```python
from pydantic import BaseModel
from measurekit import Quantity

class SensorData(BaseModel):
    temperature: Quantity
    pressure: Quantity

# Validates strings, dicts, or objects
data = SensorData(
    temperature="25 degC",
    pressure={"magnitude": 101.3, "unit": "kPa"}
)
```

---

## 🧪 Advanced Features

- **Correlation Tracking:** Uncertainties are propagated through the global `CovarianceStore`, keeping track of dependencies between measured quantities.
- **Unit Systems:** Easily define and switch between SI, Imperial, and custom unit systems.
- **Commutativity & Invariants:** Strictly follows physical laws for group operations.

### Adding Custom Units (The Transpiler)

MeasureKit uses a "Python-as-Database" approach. Instead of parsing slow TOML/YAML files at runtime, we **transpile** definitions into optimized Python modules.

1. **Define:** Add your unit to `definitions/your_scope.toml`.
   ```toml
   [volt]
   symbol = "V"
   definition = "kg * m^2 * s^-3 * A^-1"
   ```
2. **Compile:** Run the transpiler to generate the Python code.
   ```bash
   python scripts/compile_units.py
   ```
3. **Use:** Access it immediately with zero runtime parsing cost.
   ```python
   import measurekit as mk
   print(mk.units.volt)
   ```

---

## 📊 Benchmarks

MeasureKit is designed for high performance. To run the benchmark suite:

```bash
uv run pytest --benchmark-enable tests/performance/benchmarks.py
```

CI/CD automatically tracks performance regressions using `pytest-benchmark`.

## 📚 Documentation

The documentation is built with **MkDocs Material** and includes API references and interactive Jupyter tutorials.

To build and serve the documentation locally:

```bash
uv run mkdocs serve
```

Documentation includes:

- **API Reference**: Automatically generated from docstrings.
- **Tutorials**: Executable Jupyter notebooks (e.g., getting started with JAX).
- **Core Concepts**: Explanations of the Quantity and UnitSystem.

## 🤝 Contributing

Contributions are welcome! Please see our [ROADMAP.md](ROADMAP.md) for future vision and [MK-001_Best_Practices.md](MK-001_Best_Practices.md) for coding standards.

1. Fork the repository
2. Install dev dependencies: `uv sync --all-extras`
3. Run tests: `uv run pytest`
4. Submit a PR

---

## 📄 License

MeasureKit is open-source software licensed under the **MIT License**.

Built with ❤️ by **Irvin Torres** and the Scientific Python community.
