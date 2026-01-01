# MK-001: MeasureKit Best Practices & Style Guide

**Author:** Irvin Torres  
**Status:** Active  
**Type:** Informational  
**Created:** 2025-12-31 (Updated)

## 🎯 Abstract

This document outlines the architecture-aware patterns, idioms, and best practices for the `measurekit` ecosystem. It serves as the "source of truth" for developers to ensure code is not only correct but also takes full advantage of MeasureKit's high-performance multi-backend execution and strict dimensional safety.

---

## 1. High-Performance Quantity Handling

### 1.1. The `Q_` Registry Factory

The `Q_` function is the primary entry point. It handles unit parsing and backend dispatching automatically.

**Best Practice:**

```python
from measurekit import Q_
import numpy as np

# Scalar
temp = Q_(25, "degC")

# Vectorized (Automatic NumPy Backend)
array_q = Q_(np.array([1, 2, 3]), "m/s")
```

### 1.2. Utilizing the Fast Path

For performance-critical loops (e.g., simulations), avoid repeated unit conversions. Perform arithmetic on quantities with identical units to trigger the `_fast_new` optimization path, which bypasses validation.

---

## 2. Multi-Backend Development

### 2.1. Implicit Backend Dispatching

MeasureKit is designed to be "tensor-agnostic." Avoid hard-coding backend checks (`isinstance(x, torch.Tensor)`). Let the library handle it.

**Correct:**

```python
import torch
from measurekit import Q_

# Initialize with Torch - operations will automatically stay in Torch
q_torch = Q_(torch.randn(10), "kg")
result = q_torch * 2.5 # Zero-copy backend operation
```

### 2.2. Cross-Device Management

When using **PyTorch** or **JAX**, explicitly manage your devices through the quantity's `.to_device()` method.

```python
gpu_q = q.to_device("cuda:0")
```

---

## 3. Data Integrity & Validation

### 3.1. Pydantic V2 Integration

Always use `measurekit.Quantity` as a type hint in your Pydantic models to ensure input data is physically valid.

**Correct:**

```python
from pydantic import BaseModel
from measurekit import Quantity

class Experiment(BaseModel):
    duration: Quantity
    pressure: Quantity

# Validated automatically from strings
ex = Experiment(duration="10.5 s", pressure="101.3 kPa")
```

---

## 4. Uncertainty & Covariance

### 4.1. Correlated Error Propagation

MeasureKit tracks correlations automatically through a global `CovarianceStore`. Avoid manual standard deviation addition (`sqrt(s1^2 + s2^2)`).

**Correct:**

```python
a = Q_(10, "m", 0.1)
b = Q_(5, "m", 0.05)
c = a + b # Correctly propagates correlated or independent errors
```

### 4.2. Vectorized Uncertainty

For large arrays, pass the uncertainty as an array of the same shape to ensure O(N) vectorized propagation.

---

## 5. Symbolic Mathematics

### 5.1. Preserving Dimensions

Use `SymbolicQuantity` for algebraic derivations. This prevents "dimension-less" errors during complex equation solving.

```python
from measurekit.symbolic import SymbolicQuantity
force = SymbolicQuantity("F", "N")
```

---

## 6. Project Architecture (DDD)

### 6.1. Unit System Contexts

Avoid global state mutations. Use the `system_context` for localized unit system overrides.

```python
from measurekit import system_context
from measurekit.systems import IMPERIAL

with system_context(IMPERIAL):
    # Units default to Imperial within this scope
    val = Q_(1, "gallon")
```

---

## 7. Performance Checklist

| Action            | Recommended     | Performance Impact      |
| :---------------- | :-------------- | :---------------------- |
| **Instantiation** | `Q_` Factory    | High (Standard Parsing) |
| **Arithmetic**    | Same-unit ops   | **Extreme (Fast Path)** |
| **Conversions**   | `.to("unit")`   | Moderate (Matrix Mul)   |
| **Arrays**        | NumPy/Torch/JAX | **High (SIMD/GPU)**     |
| **Validation**    | Pydantic        | Moderate (Safety First) |

---

## 8. Naming Conventions

- **Variables:** Use standard physics symbols or descriptive names (`velocity_m_s` is redundant, use `velocity`).
- **Unit Strings:** Always use standard SI/Imperial abbreviations (`"kg"`, `"slug"`, `"m/s^2"`).
- **Constants:** Use `SCREAMING_SNAKE_CASE` for physical constants.
