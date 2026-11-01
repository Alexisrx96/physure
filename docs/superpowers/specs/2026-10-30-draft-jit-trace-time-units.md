# Draft: units out of the way of `torch.compile` and `physure.jit` (N4)

**Status:** DRAFT, not approved. Source: next step N4 of `2026-10-30-post-a1-inventory.md`.

## Where things stand

There are two compile paths and they are not the same thing.

- **`torch.compile` (TorchDynamo).** `Quantity` wraps a tensor and an extension object for the unit. Dynamo cannot trace into the extension's constructors and operators, so each unit operation is a graph break. Counted with `torch._dynamo.explain` (torch 2.13, CPU, Python 3.14) and written down in `docs/torch_compile_integration.md`:

  | Expression | Graph breaks | Graphs |
  |---|---|---|
  | `(a + b) * 2.0` | 6 | 7 |
  | `a * b` | 4 | 5 |
  | `a / b` | 6 | 7 |
  | `a.to('km')` | 2 | 3 |
  | `s.to('km/h')` | 2 | 3 |
  | `t.to('K')` (degC) | 2 | 3 |

  `Quantity.__init__` already tells Dynamo that `unit` and `_unit` are static while compiling (`torch._dynamo.mark_static`), and A1 kept these counts unchanged through every step.
- **`physure.jit` (the library's own tracer).** `physure/_jit/tracer.py` builds a DAG of operations, validates dimensions with the extension's `RationalUnit` (raising `DimensionalError`) and bakes a kernel; `physure/application/jit.py` is its entry point.

The existing test (`tests/ext/test_torch_compile.py`) compiles `(a + b) * 2.0` with `fullgraph=False` and checks values, unit and gradients against eager. It does not count breaks.

## Goal

A training loop over `Quantity` values whose compiled graph holds tensor operations only: dimensions checked and scale factors fixed when the function is traced, no unit objects in the loop.

## Options (all to be investigated, none is established)

- **A. Leave it.** The breaks are correct and cost compile time and fusion opportunities, not correctness.
- **B. Make unit results constants for Dynamo.** The unit of a result depends only on the units of the operands, not on tensor values, so it can be computed once per trace. Candidate mechanisms exist in torch (marking functions as non-traceable but constant, or computing the unit outside the compiled region); which of them works on current torch versions, and how they behave across the versions the project supports, has to be tried on a throwaway branch.
- **C. Use `physure.jit` as the zero-overhead path.** It already resolves dimensions at trace time and bakes a kernel; extend it to emit scale factors and hand a pure tensor function to `torch.compile` or `jax.jit`, and keep Dynamo tracing of `Quantity` as the convenience path.
- **D. Both.** B for users who just wrap their model in `torch.compile`, C for the loops that need the least overhead.

## Open questions

- Which path is the product promise: "wrap your model with `torch.compile` and it works" or "use `physure.jit` for the hot loop"?
- JAX: does the same hold under `jax.jit`? `tests/core/test_jit.py` exercises JAX, and XLA tracing treats Python objects differently from Dynamo.
- Which torch versions are in scope? Dynamo internals change between releases; the measurement above is 2.13 only, and the test is skipped on Python 3.14 below torch 2.10.

## Acceptance

- The break counts of the table go down, measured by the same script as today, and never up.
- Values and gradients of the compiled function equal eager mode.
- A training-step benchmark (compiled, with and without `Quantity`) shows the overhead removed or reports honestly that it was not the bottleneck.
- The benchmark gate of `bench_unit_ops.py` holds (no row more than 10% slower).

## Size and dependencies

Size L, risk medium (depends on torch internals, so a version bump can regress it). Independent of N1, N2, N3.
