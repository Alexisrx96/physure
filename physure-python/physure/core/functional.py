"""Functional core for Physure.

This module provides pure functions for arithmetic operations on physical quantities.
It isolates computation from object representation, enabling JIT compilation and
backend-agnostic execution via the Array API standard.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import array_api_compat
else:
    try:
        import array_api_compat
    except ImportError:
        array_api_compat = None

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.conversion_plan import apply_conversion
from physure.domain.measurement.units import (
    _CORE_ERRORS,
    CompoundUnit,
    _python_error,
)

if TYPE_CHECKING:
    from types import ModuleType

    from physure.core.protocols import Numeric
    from physure.domain.measurement.system import UnitSystem


def get_xp(*arrays: Numeric) -> ModuleType:
    """Determines the array API namespace for the given inputs.

    Args:
        *arrays: variable number of array-like objects or scalars.

    Returns:
        The array API namespace (e.g., numpy, torch, jax.numpy).
    """
    try:
        if array_api_compat:
            return array_api_compat.array_namespace(*arrays)
        raise TypeError("array_api_compat not available")
    except (TypeError, AttributeError):
        # Fallback for pure Python scalars to numpy if available
        # Also handles case where array_api_compat is None
        import numpy as xp

        return xp


def add_quantities(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    """Adds two quantities.

    Examples:
        >>> from physure.application.startup import create_system
        >>> sys = create_system()
        >>> m = sys.get_unit("m")
        >>> km = sys.get_unit("km")
        >>> res_mag, res_unit = add_quantities(10, m, 1, km, sys)
        >>> print(res_mag)
        1010.0
    """
    is_nonlinear = _check_nonlinear(unit1, unit2, system)
    if is_nonlinear:
        return _add_sub_nonlinear(val1, unit1, val2, unit2, system, False)

    # Standard Linear Path
    xp = get_xp(val1, val2)

    if unit1.dimension(system) != unit2.dimension(system):
        raise IncompatibleUnitsError(unit1, unit2)

    if unit1 != unit2:
        factor = unit2.conversion_factor_to(unit1, system)
        val2_converted = xp.multiply(val2, factor)
    else:
        val2_converted = val2

    result = xp.add(val1, val2_converted)
    return result, unit1


def sub_quantities(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    """Subtracts two quantities."""
    is_nonlinear = _check_nonlinear(unit1, unit2, system)
    if is_nonlinear:
        return _add_sub_nonlinear(val1, unit1, val2, unit2, system, True)

    xp = get_xp(val1, val2)

    if unit1.dimension(system) != unit2.dimension(system):
        raise IncompatibleUnitsError(unit1, unit2)

    if unit1 != unit2:
        factor = unit2.conversion_factor_to(unit1, system)
        val2_converted = xp.multiply(val2, factor)
    else:
        val2_converted = val2

    result = xp.subtract(val1, val2_converted)
    return result, unit1


def _check_nonlinear(
    unit1: CompoundUnit, unit2: CompoundUnit, system: UnitSystem
) -> bool:
    """True when a zero point or a logarithm is involved: not a plain scale."""
    return not (unit1.is_linear(system) and unit2.is_linear(system))


def _convert(
    xp: ModuleType,
    value: Numeric,
    source: CompoundUnit,
    target: CompoundUnit,
    system: UnitSystem,
) -> Numeric:
    """Converts a value by the plan that the core registry gives."""
    try:
        plan = system.core_registry.conversion(source, target)
    except _CORE_ERRORS as error:
        raise _python_error(error, source, target) from error
    ops = SimpleNamespace(mul=xp.multiply, add=xp.add, abs=xp.abs)
    return apply_conversion(plan, value, 0.0, ops)[0]


def _add_sub_nonlinear(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    system: UnitSystem,
    subtract: bool,
) -> tuple[Numeric, CompoundUnit]:
    """Adds or subtracts when a point (degC) or a level (dB) is involved.

    The rules are the core's: a point moved by an interval stays a point in its
    own scale, two points make an interval, levels add as the quantities they
    stand for, and what has no meaning is refused.
    """
    xp = get_xp(val1, val2)
    if unit1.dimension(system) != unit2.dimension(system):
        raise IncompatibleUnitsError(unit1, unit2)
    op = xp.subtract if subtract else xp.add
    point1 = unit1.kind(system) == "absolute"
    point2 = unit2.kind(system) == "absolute"

    if point1 or point2:
        if point1 and point2:
            if not subtract:
                raise ValueError(
                    "Cannot add two absolute quantities. "
                    "Did you mean to add a difference?"
                )
            base = unit1.base_unit(system)
            return (
                xp.subtract(
                    _convert(xp, val1, unit1, base, system),
                    _convert(xp, val2, unit2, base, system),
                ),
                base,
            )
        if point1:
            factor = unit2.conversion_factor_to(unit1, system)
            return op(val1, xp.multiply(val2, factor)), unit1
        if subtract:
            raise ValueError(
                "Cannot subtract an absolute quantity from a difference."
            )
        factor = unit1.conversion_factor_to(unit2, system)
        return xp.add(xp.multiply(val1, factor), val2), unit2

    if not (unit1.kind(system) == "delta" and unit2.kind(system) == "delta"):
        raise ValueError("Unexpected kind of unit.")
    # both are levels, since _check_nonlinear found a unit that is not a scale
    if unit1.is_linear(system) or unit2.is_linear(system):
        raise ValueError(
            f"Cannot {'subtract' if subtract else 'add'} a level and a plain "
            f"quantity ('{unit1}' and '{unit2}'): convert one of them first."
        )
    plain = unit1.base_unit(system)
    combined = op(
        _convert(xp, val1, unit1, plain, system),
        _convert(xp, val2, unit2, plain, system),
    )
    return _convert(xp, combined, plain, unit1, system), unit1


def mul_quantities(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    _system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    """Multiplies two quantities."""
    xp = get_xp(val1, val2)
    result_mag = xp.multiply(val1, val2)

    new_exponents = unit1.exponents.copy()
    for u, exp in unit2.exponents.items():
        new_exponents[u] = new_exponents.get(u, 0) + exp

    result_unit = CompoundUnit(new_exponents)
    return result_mag, result_unit


def truediv_quantities(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    _system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    """Divides two quantities."""
    xp = get_xp(val1, val2)
    result_mag = xp.divide(val1, val2)

    new_exponents = unit1.exponents.copy()
    for u, exp in unit2.exponents.items():
        new_exponents[u] = new_exponents.get(u, 0) - exp

    result_unit = CompoundUnit(new_exponents)
    return result_mag, result_unit


def pow_quantities(
    val1: Numeric, unit1: CompoundUnit, exponent: Numeric, _system: UnitSystem
) -> tuple[Numeric, CompoundUnit]:
    """Raises a quantity to a power."""
    xp = get_xp(val1)
    result_mag = xp.pow(val1, exponent)

    scalar_exp = exponent

    # Update units
    # If scalar_exp is a Tracer, we CANNOT update unit exponents (which keys must be values)
    # Unit exponents are static metadata.
    # Therefore, we CANNOT raise a Quantity to a dynamic (traced)
    # power if that power affects dimensions.
    # e.g. m^x where x is input to JIT -> m^x is not a valid static unit unless x is constant.
    # JAX JIT requires static shapes/units.
    # If x is dynamic, the unit is undefined/dynamic.
    # Physure units must be static.
    # So we MUST try to cast to float. If it fails (ConcretizationTypeError), we must fail or warn.
    # But usually power on units is integer arithmetic.
    # If user does q ** 2 (2 is const), valid.
    # If user does q ** param (param is arg), valid only if param is marked static in JAX.

    # We will try to cast to float/int for the unit logic.
    # If it fails, we let the error propagate (JAX's ConcretizationTypeError),
    # which informs the user they need to make the exponent static.
    # We can't really "fix" this for dynamic exponents because our design relies on static units.
    try:
        s_exp = float(scalar_exp)
    except Exception:
        # If we can't cast, we assume it's dynamic and valid logic is impossible for Unit exponents.
        # But maybe the operation is dimensionless?
        # If unit is dimensionless, new unit is valid (dimensionless).
        if len(unit1.exponents) == 0:
            s_exp = 1.0  # arbitrary, unit doesn't change
        else:
            raise

    new_exponents = {u: e * s_exp for u, e in unit1.exponents.items()}
    result_unit = CompoundUnit(new_exponents)
    return result_mag, result_unit
