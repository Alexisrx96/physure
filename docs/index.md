# Welcome to physure

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
- **Hand NumPy quantities, take numbers on purpose.** `np.sum`, `np.mean`, `np.dot`, `np.cross`, `np.prod`, `np.std` and the like keep the unit and the uncertainty of a list of quantities (NumPy holds the quantities in an object array). Of an array-backed quantity only `np.sum`, `np.mean`, `np.dot`/`q.dot`/`@`, `np.cross`/`q.cross`, `np.concatenate`, `np.isclose`/`np.allclose` and the elementwise ufuncs keep them (`a.dot(b)` is `np.sum(a * b)`, shared errors counted once; an uncertain `dot` beyond two dimensions or `cross` of a batch raises); `np.prod`, `np.cumsum`, `np.max`, `np.std` raise. For bare numbers write `q.to("m").magnitude` (and `q.uncertainty`): `float(q)` and `np.asarray(q, dtype=float)` give the value in the coherent unit and warn with `physure.UnitStrippedWarning` when a unit or an uncertainty is dropped. `np.asarray(q)` of an array-backed quantity builds one quantity per element (about 0.2 s for 1e5 elements): for the numbers take `q.magnitude`. `==` and `hash` compare the value in the coherent unit, so `{Q_(1, "m"), Q_(100, "cm")}` has one element; a converted value may differ in the last digit, so compare measured or converted values with `np.isclose` or `approx_eq`.
- **Let uncertainty propagate itself.** Never add standard deviations manually — correlated errors are tracked through the covariance store.
- **In hot loops, do arithmetic in identical units**: same-unit operations take a fast path that bypasses parsing and validation.
- **Validate with Pydantic** by annotating model fields as `Quantity`; strings like `"101.3 kPa"` are parsed and dimension-checked on model construction.
- **Scope unit-system overrides** with `system_context(...)` instead of mutating global state.
- **With `torch.compile`**, prefer `fullgraph=True` and keep symbolic tracing out of compiled functions — see [torch.compile integration](torch_compile_integration.md).
