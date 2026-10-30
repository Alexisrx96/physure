# Units out of the way of `torch.compile` (N4) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer ("Proceed", 2026-10-30) as three stages; S1 in progress on `feat/jit-trace-time-units`, S2 and S3 not started.
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

## Acceptance

- The break counts of the table go down, measured by `benchmarks/count_graph_breaks.py`, and never up; the baseline in `test_graph_breaks.py` is lowered in the same change that lowers a count.
- Compiled values, units and gradients equal eager mode.
- `bench_unit_ops.py` holds its gate.
- On a torch without the S2 mechanism the behaviour is exactly today's.

## Not in scope

`physure.jit` and JAX (`jax.jit`), another torch backend than `aot_eager` for the tests, and `fullgraph=True` as a promise: it is the direction, not a gate of this change.
