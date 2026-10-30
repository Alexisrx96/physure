# Torch Compile Integration

**Last Updated:** October 29, 2026

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

## 2. Units and Dimensions Are Objects of the Extension

A unit is a `RationalUnit` of the Rust extension (`CompoundUnit` is a subclass of it) and a `Dimension` holds one; their algebra, parsing and conversion plans are the core's. Dynamo cannot trace into the constructors and operators of an extension type, so every operation on a unit on the compiled path is a graph break. What stays in the graph is the arithmetic on the tensors between those calls.

The breaks are counted with `torch._dynamo.explain` (torch 2.13, CPU, Python 3.14) on quantities of tensors:

| Expression | Graph breaks | Graphs |
|---|---|---|
| `(a + b) * 2.0` | 6 | 7 |
| `a * b` | 4 | 5 |
| `a / b` | 6 | 7 |
| `a.to('km')` | 2 | 3 |
| `s.to('km/h')` | 2 | 3 |
| `t.to('K')` (degC) | 2 | 3 |

These counts are the gate of the unit work (A1): moving the unit algebra, the conversion plans, the point/level arithmetic and the exact powers into the core left each of them where it was. A change that makes one larger is a regression, whatever it speeds up.

## 3. FakeTensors & Backend Operations

TorchInductor uses `FakeTensor` (meta-tensors without data) to propagate shapes and types. Our backend implementation (`torch_backend.py`) had issues:

- **`size()`**: Using `.size` on a `FakeTensor` works, but `.numel()` is safer and preferred by Torch internals for symbolic shape propagation. We switched to `torch.numel(obj)`.
- **`reshape()`**: Checks for `.is_sparse` on a `FakeTensor` must be handled carefully. We use `getattr(obj, "is_sparse", False)` and ensure we don't trip over symbolic boolean checks.

## 4. Verification

`tests/ext/test_torch_compile.py` compiles `(a + b) * 2.0` over two quantities with `backend="aot_eager"` and checks the compiled result against the eager one: the magnitude, the unit and the gradients.
Command: `uv run pytest tests/ext/test_torch_compile.py`

It uses `aot_eager` to verify the capture without a C++ compiler (MSVC) on Windows CI, and `fullgraph=False`: the constructors of the extension are graph breaks (section 2), which `fullgraph=True` would reject. It does not count the breaks; the table above does.
