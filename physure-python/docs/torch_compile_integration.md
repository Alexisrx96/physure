# Torch Compile Integration

**Last Updated:** January 7, 2026

This document details the strategies used to make `physure.Quantity` compatible with `torch.compile` (TorchDynamo/Inductor).

## 1. Graph Breaks & ContextVars

TorchDynamo attempts to trace Python execution into a graph. It strictly forbids calls to `contextvars.ContextVar.get()` during tracing because they introduce side effects that are hard to capture symbolically.

**The Fix:**
We modified `physure.application.tracing.context.get_active_tracer` and `physure.domain.measurement.quantity.Quantity.from_input` to check `torch.compiler.is_compiling()`:

```python
if torch.compiler.is_compiling():
    # Disable Physure's ContextVar-based features (tracing, uncertainty mode config)
    # Default to safe static values
    return None
```

This effectively "inlines" the `Quantity` logic as pure Python operations without global state lookups during compilation.

## 2. Frozen Dataclasses & In-Place Mutation

`physure.domain.measurement.dimensions.Dimension` is a frozen dataclass. In `units.py`, we previously used in-place multiplication:

```python
overall *= system.UNIT_DIMENSIONS[unit] ** exp
```

Dynamo flagged this as an unsupported mutation of a frozen class or a complex side-effect. We changed this to explicit assignment:

```python
overall = overall * (system.UNIT_DIMENSIONS[unit] ** exp)
```

This is friendlier to the FX graph capture.

## 3. FakeTensors & Backend Operations

TorchInductor uses `FakeTensor` (meta-tensors without data) to propagate shapes and types. Our backend implementation (`torch_backend.py`) had issues:

- **`size()`**: Using `.size` on a `FakeTensor` works, but `.numel()` is safer and preferred by Torch internals for symbolic shape propagation. We switched to `torch.numel(obj)`.
- **`reshape()`**: Checks for `.is_sparse` on a `FakeTensor` must be handled carefully. We use `getattr(obj, "is_sparse", False)` and ensure we don't trip over symbolic boolean checks.

## 4. Verification

We added `tests/ext/test_torch_compile.py`.
Command: `uv run pytest tests/ext/test_torch_compile.py`

This test uses `backend="aot_eager"` to verify graph capture without requiring a C++ compiler (MSVC) on Windows CI environments, while `fullgraph=True` ensures no graph breaks occur.
