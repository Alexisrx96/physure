<p align="center">
  <img src="MeasureKitLogoBeta.jpg" alt="MeasureKit Logo" width="400">
</p>

<h1 align="center">MeasureKit</h1>

<p align="center">
  <strong>A powerful Python library for physical computing, unit conversions, and symbolic analysis.</strong>
</p>

<p align="center">
  <a href="https://pypi.org/project/measurekit/">
    <img src="https://img.shields.io/pypi/v/measurekit.svg" alt="PyPI version">
  </a>
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT">
  </a>
  <a href="https://www.python.org/">
    <img src="https://img.shields.io/badge/python-3.8+-blue.svg" alt="Python 3.8+">
  </a>
</p>

---

## 🚀 Description

**MeasureKit** is designed to make working with physical quantities in Python intuitive, robust, and error-free. Whether you are a scientist, engineer, or student, MeasureKit handles the complexity of unit conversions and dimensional analysis so you can focus on the physics.

## 📦 Installation

Install the package via pip:

```bash
pip install measurekit
```

To use it within a Jupyter Notebook/Lab environment:

```bash
pip install ipykernel
python -m ipykernel install --user --name measurekit
```

## 🛠 Usage

```python
from measurekit import Q_

# 1. Easy Unit Conversions
# Use the Q_ factory to create quantities
feet = Q_(3.28, 'ft')
meters = feet.to('m')
print(f"3.28 feet = {meters:.4f}")

# 2. Quantity Arithmetic
distance = Q_(5, 'km')
time = Q_(2, 'h')

# Automatic unit handling in calculations
speed = distance / time
print(f"Speed: {speed}")  # Output: 2.5 km/h

# 3. Fluid Conversions
speed_in_mph = speed.to('mph')
print(f"Speed in mph: {speed_in_mph}")
```

## ✨ Features

- **Robust Unit System:** Support for SI, Imperial, and custom unit definitions.
- **Dimensional Analysis:** Prevents invalid operations (e.g., adding length to mass) at runtime.
- **Symbolic Math:** Built-in support for symbolic manipulation of physical equations.
- **Extensible:** Easily define your own units and constants via configuration files.
- **Developer Friendly:** Type hints, clean API, and comprehensive error messages.

## ⚙️ Configuration

MeasureKit allows you to customize its behavior, define new units, and add constants through a `measurekit.conf` file.

1.  **Create a `measurekit.conf` file** in your project's root directory.
2.  **Define your custom settings** following the INI format.

**Example `measurekit.conf`:**

```ini
[Settings]
default_system = SI

[Units]
# Format: name = factor_to_base, dimension, [symbol, alias...]
hand = 0.1016, L, [hand, hands]
bitcoin = 95000.0, $, [BTC, bitcoin]

[Constants]
# Format: name = value unit
my_gravity = 9.81 m/s^2
```

## 📋 Requirements

- Python 3.8 or higher
- `sympy`
- `numpy` (optional, for advanced features)
- `scipy` (optional, for simulations)

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## ✍️ Author

**Irvin Torres** - [irvinrx1996@hotmail.com](mailto:irvinrx1996@hotmail.com)
