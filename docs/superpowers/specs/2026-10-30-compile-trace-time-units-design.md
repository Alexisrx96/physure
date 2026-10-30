# Units out of the way of `torch.compile` (N4) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer ("Proceed", 2026-10-30) as three stages. S1 was tried and **did not move the counts** (see "S1 outcome"); what is built is the measurement and the gate. The benchmark (below) shows the breaks are not the bottleneck, so S2 is parked.
**Base:** local `main` @ `0747d64`
**Follows:** the draft `2026-10-30-draft-jit-trace-time-units.md` (next step N4 of `2026-10-30-post-a1-inventory.md`).

## Decisions (maintainer, 2026-10-30)

| Question | Decision | Rejected |
|---|---|---|
| Which path is the promise | **`torch.compile` first**: wrapping a model in `torch.compile` should work with few breaks; `physure.jit` is left as it is | `physure.jit` first; both at once |
| Torch versions (the extra says `torch>=2.2.0`, counts were measured on 2.13 only) | **Newest gains, older unchanged**: a mechanism that exists only in new torch is feature-tested at import; on older torch behaviour is exactly today's | raising the floor; a Python fallback for old torch (it would duplicate the Rust unit algebra) |
| Scope | **Everything on the path**: unit objects, the `ContextVar` read and the data-dependent `any(mask)` check | unit objects only |

## What was measured (torch 2.13.0+cpu, Python 3.14.6, `main` @ `0747d64`)

`benchmarks/count_graph_breaks.py` (new) counts the breaks of six expressions with `torch._dynamo.explain`; `tests/ext/test_graph_breaks.py` (new) holds the baseline and fails when a count goes up.

| Expression | Breaks | Graphs |
|---|---|---|
| `(a + b) * 2.0` | 6 | 7 |
| `a * b` | 4 | 5 |
| `a / b` | **7** | 8 |
| `a.to('km')` | 2 | 3 |
| `s.to('km/h')` | 2 | 3 |
| `t.to('K')` | 2 | 3 |

`docs/torch_compile_integration.md` says 6 for `a / b`; the count is stable at 7 across runs, so the document was already one off (not bisected: whether F5 raised it is unknown).

Every break traced to its physure source line (`TORCH_LOGS=graph_breaks`):

| Cause | Where | Kind |
|---|---|---|
| `UnitRegistry.kind_of` | `domain/measurement/units.py:232`, from `_is_absolute()` in every `*` and `/` | call into a Rust object |
| `UnitRegistry.dimension_of` | `units.py:112`, from `Quantity.__post_init__` on every creation | call into a Rust object |
| `RationalUnit.__new__` | `quantity.py:201` (`_CoreRationalUnit(dims)` in `Quantity.__new__`) and `quantity.py:258` (`getattr(u, "dimensions")` on the core's unit in `__init__`) | construction of a Rust object |
| `parse_unit_expression` | `application/parsing.py:24` (a unit string in `.to('km')`) | call into a Rust function |
| `ContextVar.get` | `application/context.py:272` (`get_uncertainty_model`) | Python state |
| `Tensor.item()` | `backends/torch_backend.py:265` from `if not backend.any(mask)` in `Uncertainty.from_standard` (`uncertainty.py:127`) | a branch on tensor data |

## Spike: torch's opaque value types (S2 candidate)

`torch._library.opaque_object.register_opaque_type(cls, typ="value", members={...: MemberType.USE_REAL})` makes instances of `cls` compile-time constants and evaluates the listed members with the real object. On torch 2.13, with `CompoundUnit` registered: `if m == s:` compiles with `fullgraph=True` (a constant branch) and `m * s` traces. What it needs: a real `__fx_repr__` (an evaluable representation, because a unit that ends up a graph output is written into the generated code) and non-default `__eq__`/`__hash__` on every registered class; `Dimension` and `UnitSystem` must be registered too (they are arguments and results of `kind(system)` and `dimension(system)`); `UnitSystem` has a custom `__getattr__`, which Dynamo refuses for opaque types. It is a private API (`torch._library`) and its first torch version is not checked.

## Stages

- **S1 — Python-only causes** (any torch version, no private API): read `get_uncertainty_model` without `ContextVar.get()` while `torch.compiler.is_compiling()`, as the tracer context already does; make the `any(mask)` check not branch on tensor data while compiling; remove avoidable Rust-object construction from `Quantity` creation where it is not needed. Each is measured on its own against the baseline.
- **S2 — Rust-object calls** (`kind_of`, `dimension_of`, `RationalUnit.__new__`, `parse_unit_expression`): a time-boxed spike of two mechanisms, then a go/no-go report to the maintainer before anything is built on it. (i) The opaque value types above, feature-tested, needing `__fx_repr__`s, `Dimension`/`UnitSystem` registration and a `__getattr__`-free `UnitSystem` on the compiled path. (ii) A table of unit-algebra results keyed by the identity of the operand units, filled by one disabled call and read as constants afterwards (works on any torch; costs one recompile on warm-up).
- **S3 — verification**: the six-expression table before and after, values and gradients of compiled equal to eager (`tests/ext/test_torch_compile.py`), the `bench_unit_ops.py` gate (no row more than 10 % slower), a training-step benchmark (compiled, with and without `Quantity`) that either shows the overhead removed or says honestly that it was not the bottleneck, and the table in `docs/torch_compile_integration.md` corrected.

## S1 outcome (2026-10-30): the counts do not move

S1 was tried one cause at a time, measured after each. Mirroring `get_uncertainty_model` for compiled code (a module global that `uncertainty_model` sets next to the `ContextVar`, which Dynamo reads as a constant) removed that break from the log of `(a + b) * 2.0`, and the count stayed at 6: the next read on the same path, `get_propagation_mode` (`context.py:325`), broke in its place. The mirror was reverted because it added behaviour (a global that is not isolated per thread) and bought nothing measurable.

Why piecemeal fixes cannot lower the headline numbers:

- **Every `Quantity` creation pays the Rust-object breaks.** `a * b` creates one quantity and breaks 4 times; `(a + b) * 2.0` creates two and breaks 6; `a / b` breaks 7. `RationalUnit.__new__` (twice), `dimension_of`, `kind_of` and, for `.to`, `parse_unit_expression` are called on each one. While they stay, removing a Python-state break only moves the break to the next unsupported call.
- **More `ContextVar` reads sit on the same path**: `get_propagation_mode`, the covariance store (`vectorized_uncertainty._current_store`), the equivalencies, and `get_current_system`, which is already a deliberate `torch.compiler.disable` (itself a break). Each needs the same treatment before any of them pays off.
- **The `any(mask)` branch cannot be removed by a type check.** Choosing `VarianceModel` or `CovarianceModel` by whether any element of the standard deviation is non-zero is a decision about data. Dropping it while compiling would change results silently (an uncertain array would lose its correlation tracking inside the compiled region, so `x - x` would stop cancelling). A safe version needs an "exactly zero" fact carried by the uncertainty object, set where the zero is known to be a Python scalar, and used by `from_input` and `_convert_by_plan`. That is a change to the uncertainty classes, not a local edit.

So the Python-state fixes are prerequisites of a zero-break compiled graph, not steps toward a lower count on their own. The count falls only when S2 removes the Rust-object calls.

## Benchmark: are the breaks the bottleneck? (2026-10-30)

`benchmarks/bench_compile_training_step.py`: one forward and backward pass of a small physics loss (`e = 0.5 * m * (d / t)**2`, squared error against a target in joules, `.mean()`), as plain tensors and as `Quantity`, eager and `torch.compile(backend="aot_eager")`. torch 2.13.0+cpu, Python 3.14.6; median of 7 batches of 20 steps. Inductor is not available on this machine (no C++ compiler), so kernel fusion was not measured.

| n | plain eager | plain compiled | `Quantity` eager | `Quantity` compiled | compile warm-up (`Quantity`) |
|---|---|---|---|---|---|
| 100 | 0.08 ms | 0.19 ms | 1.7 ms (22x) | 4.2 ms (55x) | 4.3 s |
| 1,000 | 0.08 ms | 0.21 ms | **89.6 ms (1,163x)** | 94.7 ms | 4.7 s |
| 4,000 | 0.16 ms | 0.25 ms | **1,044 ms (6,446x)** | 1,046 ms | 5.7 s |

What it says:

- **The graph breaks are not the bottleneck.** Compiling the `Quantity` step makes it slower at n = 100 (4.2 ms against 1.7 ms: guards and resume functions around four breaks per quantity) and changes nothing at n = 1,000 and above. The most that removing every break could recover is that 2.5 ms at small sizes, plus the multi-second warm-up.
- **The cost is quadratic in the array length, and it is in eager mode.** With torch tensors, `Quantity * Quantity` and `Quantity / Quantity` build a dense `N x N` diagonal Jacobian (`_arithmetic_mixin._mul_quantities` and `_div_quantities` call `backend.diagonal_operator`, which for torch is `torch.diag`). Timing a single multiply: 23 ms at 1,000 elements, 73 ms at 2,000, 229 ms at 4,000, 491 ms at 8,000, 2,046 ms at 16,000. A million elements would ask for 4 TB, and the benchmark died there with `DefaultCPUAllocator: not enough memory`. The numpy backend uses `scipy.sparse.diags` for the same operator and is flat (0.8 ms at 16,000 elements).
- Even with the exact quantities of this benchmark (no uncertainty) the Jacobians are built, because nothing short-circuits on a zero uncertainty.

So for the use that motivated N4, training loops over `Quantity` tensors, the unit bookkeeping costs three to four orders of magnitude more than the graph breaks do, and for a reason unrelated to units or Dynamo. S2 is not worth starting before that is fixed. The fix is a separate item (a sparse diagonal for torch, as numpy has, or skipping the Jacobian when the uncertainties are exactly zero).

## Acceptance

- The break counts of the table go down, measured by `benchmarks/count_graph_breaks.py`, and never up; the baseline in `test_graph_breaks.py` is lowered in the same change that lowers a count.
- Compiled values, units and gradients equal eager mode.
- `bench_unit_ops.py` holds its gate.
- On a torch without the S2 mechanism the behaviour is exactly today's.

## Not in scope

`physure.jit` and JAX (`jax.jit`), another torch backend than `aot_eager` for the tests, and `fullgraph=True` as a promise: it is the direction, not a gate of this change.
