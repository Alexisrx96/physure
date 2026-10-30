"""`physure.jit`: unit-aware JIT for plain Python functions.

Traces the function to validate units, then bakes a unit-less numerical kernel.
The tracing primitives live in `physure._jit.tracer`.
"""

from __future__ import annotations

import functools
from typing import TYPE_CHECKING, Any

from physure._jit.tracer import (
    _JIT_CACHE,
    _build_unit_sig,
    _rational_unit_to_exponents,
    _trace_and_bake,
)
from physure.application.context import get_uncertainty_mode
from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import CompoundUnit, get_default_system

if TYPE_CHECKING:
    from collections.abc import Callable


def jit(func: Callable) -> Callable:
    """JIT compiler for unit-aware functions.

    Traces the function to validate units and bakes a unit-less numerical kernel.
    """

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        unit_sig = _build_unit_sig(args)
        sig = (func, tuple(unit_sig))

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        if sig not in _JIT_CACHE:
            result = _trace_and_bake(func, args, unit_sig, kwargs)
            if result is None:
                return func(*args, **kwargs)
            _JIT_CACHE[sig] = result

        kernel, out_r_unit = _JIT_CACHE[sig]
        magnitudes = [getattr(a, "magnitude", a) for a in args]
        raw_res = kernel(*magnitudes)

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        exponents = _rational_unit_to_exponents(out_r_unit)
        out_unit = CompoundUnit(exponents)
        return Quantity.from_input(raw_res, out_unit, get_default_system())

    return wrapper
