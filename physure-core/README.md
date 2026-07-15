# Physure Core 🦀

[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

The high-performance core engine for **Physure**, providing robust unit management, physical quantities with uncertainty propagation, and seamless Python integration.

## ✨ Key Features

- **🎯 Exact Unit Representation**: Uses rational exponents to eliminate floating-point errors in dimensional analysis.
- **🎲 Uncertainty Propagation**: Deep support for GUM-compliant uncertainty handling, including Gaussian, Monte Carlo, and Unscented transforms.
- **📊 Optimized Data Structures**: Built on `ndarray` and `nalgebra` for high-performance numerical operations.
- **🚀 Python Bindings**: Native-speed performance for Python users via PyO3 and Maturin.
- **📦 Arrow Support**: Direct serialization to Apache Arrow for efficient data interchange.

## 🛠 Tech Stack

- **Rust**: The memory-safe, high-performance base.
- **PyO3 & Maturin**: Powering the Python interface.
- **Num-Rational**: For precise unit arithmetic.
- **Apache Arrow**: For cross-language data handling.
- **Ndarray & Nalgebra**: For advanced mathematical operations.

## 🚀 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Python 3.10+](https://www.python.org/downloads/)
- [Maturin](https://github.com/PyO3/maturin)

### Installation

For development and local testing:

```bash
# Clone the repository
git clone https://github.com/Alexisrx96/physure.git
cd physure/physure_core

# Build and install the develop version
maturin develop
```

## 📖 Quick Example (Python)

```python
from physure_core import Quantity, RationalUnit

# Define units (e.g., meters)
meter = RationalUnit({"m": 1})

# Create quantities with uncertainty
length = Quantity(10.0, unit=meter, uncertainty=0.1)
width = Quantity(5.0, unit=meter, uncertainty=0.05)

# Automatic unit and uncertainty propagation
area = length * width

print(f"Area: {area.mean} ± {area.std_dev} {area.unit}")
# Output: Area: 50.0 ± 0.7071... m^2
```

## 🏗 Project Structure

- `src/units.rs`: Rational unit system and registry.
- `src/quantity.rs`: Physical quantity implementation with backends.
- `src/uncertainty.rs`: Uncertainty propagation logic (Gaussian, MC, etc.).
- `src/covariance.rs`: Advanced covariance management.
- `src/serialization.rs`: Arrow and memory serialization.

## 📄 License

This project is licensed under the **MIT License**. See [LICENSE](LICENSE) for details.
