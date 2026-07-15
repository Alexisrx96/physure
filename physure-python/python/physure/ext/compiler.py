"""Ahead-of-time physics-model compilation (roadmap §7).

Traces a plain Python function once over unit-annotated placeholder
arguments to statically verify dimensions, then bakes the resulting DAG
into a flat, unit-stripped kernel via the existing
`physure._jit.tracer` machinery (the same trace/strip/bake pipeline
`physure.jit` uses, minus the `Quantity` re-wrapping).

# ponytail: `target` is accepted for API parity with the roadmap's
# "JIT(LLVM/Wasm) or AOT(C/Rust)" proposal but is not dispatched on — an
# LLVM/Wasm backend would need a new runtime dependency, which violates
# CLAUDE.md's zero-dependency policy. The baked, exec()-compiled Python
# kernel (no per-call unit checks or Quantity wrapping) already satisfies
# the "strip units, run raw floats" requirement; add real LLVM/Wasm
# codegen only if profiling shows the exec'd kernel itself is the
# bottleneck.
"""

from __future__ import annotations

import inspect
from typing import TYPE_CHECKING, Any

from physure._jit.tracer import (
    TracerQuantity,
    _ensure_rational,
    bake_kernel,
)
from physure.domain.measurement.units import get_default_system

if TYPE_CHECKING:
    from collections.abc import Callable


def compile_physics_model(
    func: Callable[..., Any],
    input_units: dict[str, str],
    output_unit: str,
    target: str = "llvm",
) -> Callable[..., float]:
    """Compiles a unit-checked formula into a raw-float kernel.

    Traces `func` once over dimensioned placeholders to statically verify
    that every operation inside it is dimensionally consistent, then
    returns a callable that operates on raw floats with no per-call unit
    overhead.

    Raises:
        DimensionalError: `func` combines mismatched units internally.
        ValueError: the traced output unit doesn't match `output_unit`.

    >>> def drag_force(density, velocity, area, drag_coeff):
    ...     return 0.5 * density * (velocity**2) * area * drag_coeff
    >>> fast_drag = compile_physics_model(
    ...     drag_force,
    ...     input_units={
    ...         "density": "kg/m^3",
    ...         "velocity": "m/s",
    ...         "area": "m^2",
    ...         "drag_coeff": "",
    ...     },
    ...     output_unit="N",
    ... )
    >>> round(fast_drag(1.225, 10.0, 2.5, 0.3), 4)
    45.9375
    """
    system = get_default_system()
    param_names = list(inspect.signature(func).parameters)

    tracer_args = {
        name: TracerQuantity(
            name, _ensure_rational(system.resolve_unit(input_units[name]))
        )
        for name in param_names
    }
    trace_result = func(**tracer_args)
    if not isinstance(trace_result, TracerQuantity):
        raise TypeError(
            "compile_physics_model requires func to return a dimensioned "
            "expression built from its arguments"
        )

    expected_unit = _ensure_rational(system.resolve_unit(output_unit))
    if trace_result.unit != expected_unit:
        raise ValueError(
            f"compile_physics_model: traced output unit "
            f"{trace_result.unit} does not match declared "
            f"output_unit {output_unit!r}"
        )

    return bake_kernel(trace_result.node, param_names)
