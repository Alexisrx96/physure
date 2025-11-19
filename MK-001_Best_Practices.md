# MK-001: MeasureKit Best Practices & Style Guide

**Author:** Irvin Torres  
**Status:** Active  
**Type:** Informational  
**Created:** 2025-11-19

## Abstract

This document outlines the recommended patterns, idioms, and best practices for using the `measurekit` library. It serves as a guide for developers to write clean, correct, and efficient code using MeasureKit, ensuring consistency across projects.

## 1. Quantity Creation

### 1.1. Use the `Q_` Factory

The preferred method for creating quantities is the `Q_` factory function. It provides a unified interface for parsing values, units, and uncertainty.

**Correct:**

```python
from measurekit import Q_

length = Q_(10, "m")
speed = Q_(25, "m/s")
mass_with_error = Q_(50, "kg", 0.5) # 50 ± 0.5 kg
```

**Avoid:**
Direct instantiation of `Quantity` classes unless you are developing internal extensions to the library.

## 2. Unit Systems & Contexts

### 2.1. Context Managers for System Switching

When working with non-default unit systems (like Imperial), use the `system_context` manager. This ensures that all quantities created within the block belong to the specified system and that the global state is cleanly reverted afterwards.

**Correct:**

```python
from measurekit import system_context, Q_
from measurekit.infrastructure.config.systems import imperial

with system_context(imperial):
    weight = Q_(150, "lb")
    height = Q_(6, "ft")
    # Calculations here use Imperial rules
```

### 2.2. Explicit System Definition

When defining a custom system, register dimensions and units explicitly before use.

## 3. Symbolic Mathematics

### 3.1. Use `SymbolicQuantity` and `Equation`

For algebraic manipulation, use `SymbolicQuantity` instead of raw SymPy symbols. This preserves unit information during symbolic solving.

**Correct:**

```python
from measurekit.symbolic import SymbolicQuantity, Equation

E = SymbolicQuantity("E", "J")
m = SymbolicQuantity("m", "kg")
c = SymbolicQuantity("c", "m/s")

# Explicitly pass variables to the Equation
eq = Equation(E, m * c**2, variables=[E, m, c])
mass_expr = eq.solve_for("m")
```

## 4. Dynamics & Simulation

### 4.1. Encapsulate Physics in `Function`

Use the `Function` class to define physical laws. This ensures dimensional consistency and allows for easy reuse in simulations.

**Correct:**

```python
from measurekit.functions import Function

# Define a function with explicit dimensions
kinetic_energy = Function(
    parameters={'m': MASS, 'v': VELOCITY},
    output_dimension=ENERGY,
    symbolic_func=0.5 * m * v**2
)
```

### 4.2. Use Unit-Aware Solvers

For differential equations, use `solve_unit_aware_ivp` rather than raw `scipy.integrate`. This wrapper handles unit stripping and re-wrapping automatically.

## 5. Interoperability & Plotting

### 5.1. Access `.magnitude` for External Libraries

Libraries like Matplotlib and NumPy do not understand `Quantity` objects. Always extract the magnitude (and ensure units are consistent) before passing data to them.

**Correct:**

```python
import matplotlib.pyplot as plt

# Ensure the array is in the desired unit first
time_vals = result.t.to("s").magnitude
pos_vals = result.y[0].to("m").magnitude

plt.plot(time_vals, pos_vals)
```

## 6. Uncertainty Handling

### 6.1. Explicit Uncertainty

Pass uncertainty as the third positional argument to `Q_`.

**Correct:**

```python
reading = Q_(10.5, "V", 0.1) # 10.5 ± 0.1 V
```

## 7. Naming Conventions

- **Quantities:** Use descriptive variable names (e.g., `velocity`, `initial_mass`).
- **Units:** Use standard abbreviations in strings (e.g., `"m/s"`, `"kg"`).
- **Dimensions:** Use uppercase constants for dimensions (e.g., `LENGTH`, `TIME`).
