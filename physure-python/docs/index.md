> **⚠️ Deprecation Notice (v0.1.9):** `physure` will be renamed to **`physure`** in v0.2.0. The API is identical. See the [Migration Guide](migration.md).

# Welcome to Physure *(deprecated → physure)*

Physure is a high-performance physical dimension handling and unit conversion engine designed for modern scientific computing. It features **multi-backend support** (NumPy, JAX, PyTorch, Python), **static type safety** with `jaxtyping`, and **Pydantic integration**.

## Key Features

- **Multi-Backend Support**: Seamlessly switch between NumPy, JAX, PyTorch, and Python backends.
- **Type Safety**: Strictly typed tensor operations using `jaxtyping`.
- **Performance**: Optimized for speed with vectorized operations.
- **Pydantic Integration**: Easy validation and serialization using Pydantic V2.
- **Uncertainty Propagation**: Built-in support for handling measurement uncertainties.

## Installation

```bash
pip install physure
```

## Quick Start

```python
from physure import Q_

q = Q_(10, "m")
q_km = q.to("km")

# With uncertainty — correlations propagate automatically
a = Q_(10, "m", 0.1)
b = Q_(5, "m", 0.05)
c = a + b
```

## Best Practices

- **Use the `Q_` factory** for everything: it parses units and dispatches backends automatically (`Q_(np.array([1, 2, 3]), "m/s")` stays in NumPy; a torch tensor stays in torch).
- **Let uncertainty propagate itself.** Never add standard deviations manually — correlated errors are tracked through the covariance store.
- **In hot loops, do arithmetic in identical units**: same-unit operations take a fast path that bypasses parsing and validation.
- **Validate with Pydantic** by annotating model fields as `Quantity`; strings like `"101.3 kPa"` are parsed and dimension-checked on model construction.
- **Scope unit-system overrides** with `system_context(...)` instead of mutating global state.
- **With `torch.compile`**, prefer `fullgraph=True` and keep symbolic tracing out of compiled functions — see [torch.compile integration](torch_compile_integration.md).
