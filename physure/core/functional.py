"""Functional core for Physure.

This module provides pure functions for arithmetic operations on physical quantities.
It isolates computation from object representation, enabling JIT compilation and
backend-agnostic execution via the Array API standard.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

try:
    import array_api_compat
except ImportError:
    array_api_compat = None


from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)
from physure.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from types import ModuleType

    from physure.core.protocols import Numeric
    from physure.domain.measurement.dimensions import Dimension
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


def _get_converter_if_simple(unit: CompoundUnit, system: UnitSystem):
    """Returns the converter if the unit is a single simple unit."""
    if len(unit.exponents) == 1:
        name, exp = next(iter(unit.exponents.items()))
        if exp == 1:
            definition = system.get_definition(name)
            if definition:
                return definition.converter
    return None


def _find_linear_unit_for_dimension(
    dimension: Dimension, system: UnitSystem
) -> CompoundUnit | None:
    """Finds a linear unit (scale=1.0) for the given dimension."""
    # This searches the registry for a base unit or standard linear unit
    # to return results of difference operations (e.g. T - T)
    for name, u_def in system.UNIT_REGISTRY.get(dimension, {}).items():
        if isinstance(u_def.converter, LinearConverter) and math.isclose(
            u_def.converter.scale, 1.0
        ):
            return system.get_unit(name)
    return None


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
        return _add_nonlinear(val1, unit1, val2, unit2, system)

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
        return _sub_nonlinear(val1, unit1, val2, unit2, system)

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
    conv1 = _get_converter_if_simple(unit1, system)
    conv2 = _get_converter_if_simple(unit2, system)

    nl1 = conv1 and not conv1.is_linear
    nl2 = conv2 and not conv2.is_linear
    return bool(nl1 or nl2)


def _add_nonlinear(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    xp = get_xp(val1, val2)
    conv1 = _get_converter_if_simple(unit1, system)
    conv2 = _get_converter_if_simple(unit2, system)

    # Check dimensions
    if unit1.dimension(system) != unit2.dimension(system):
        raise IncompatibleUnitsError(unit1, unit2)

    is_off1 = isinstance(conv1, OffsetConverter)
    is_off2 = isinstance(conv2, OffsetConverter)

    # 1. Offset + Offset -> Error
    if is_off1 and is_off2:
        raise ValueError(
            "Cannot add two affine quantities (e.g. Temperatures)."
        )

    # 2. Linear + Offset -> T (Offset)
    if not is_off1 and is_off2:
        # Delta + T -> T
        # Convert both to base, add, convert back to unit2
        # Delta to base: val1 * scale
        base1 = (
            xp.multiply(val1, getattr(conv1, "scale", 1.0)) if conv1 else val1
        )  # Default linear
        base2 = conv2.to_base(val2)
        res_base = xp.add(base1, base2)
        res_mag = conv2.from_base(res_base)
        return res_mag, unit2

    # 3. Offset + Linear -> T (Offset)
    if is_off1 and not is_off2:
        base1 = conv1.to_base(val1)
        base2 = (
            xp.multiply(val2, getattr(conv2, "scale", 1.0)) if conv2 else val2
        )
        res_base = xp.add(base1, base2)
        res_mag = conv1.from_base(res_base)
        return res_mag, unit1

    # 4. Log + Log (Power Sum)
    is_log1 = isinstance(conv1, LogarithmicConverter)
    is_log2 = isinstance(conv2, LogarithmicConverter)

    if is_log1 and is_log2:
        base1 = conv1.to_base(val1)
        base2 = conv2.to_base(val2)
        res_base = xp.add(base1, base2)
        # Convert back to unit1 (arbitrary choice, usually units match for log sum)
        res_mag = conv1.from_base(res_base)
        return res_mag, unit1

    # Fallback to linear if we missed a case or mixed types essentially treated as linear
    # This effectively recurses to linear behavior manually
    # But for safety, we implement the conversion here if mixed types fell through
    # (e.g. one Log, one Linear? usually undefined or strict error,
    # but let's assume a valid linear-ish op)
    # Ideally should not happen if _check_nonlinear was accurate.
    # We'll just define it as "Linear fallback"
    factor = unit2.conversion_factor_to(unit1, system)
    val2_converted = xp.multiply(val2, factor)
    return xp.add(val1, val2_converted), unit1


def _sub_nonlinear(
    val1: Numeric,
    unit1: CompoundUnit,
    val2: Numeric,
    unit2: CompoundUnit,
    system: UnitSystem,
) -> tuple[Numeric, CompoundUnit]:
    xp = get_xp(val1, val2)
    conv1 = _get_converter_if_simple(unit1, system)
    conv2 = _get_converter_if_simple(unit2, system)

    if unit1.dimension(system) != unit2.dimension(system):
        raise IncompatibleUnitsError(unit1, unit2)

    is_off1 = isinstance(conv1, OffsetConverter)
    is_off2 = isinstance(conv2, OffsetConverter)

    # 1. Offset - Offset -> Delta (Linear)
    if is_off1 and is_off2:
        base1 = conv1.to_base(val1)
        base2 = conv2.to_base(val2)
        res_base = xp.subtract(base1, base2)

        # Result needs to be in a linear unit of the same dimension (e.g. Kelvin)
        # Try to find one
        target_unit = _find_linear_unit_for_dimension(
            unit1.dimension(system), system
        )
        if not target_unit:
            # Dangerous fallback: use unit1 but it is Offset?
            # Theoretically we have a value in Base Units.
            # If we wrap it in unit1 (Celcius), 10 K becomes 10 C? No.
            # We must return it in base unit if we have it, or synthesize.
            # Assuming system has base units setup correctly for dimensions.
            # If search fails, we can't represent it linearly easily?
            # Let's hope logic holds.
            # Fallback: Just return value and unit1, but user must know it's raw base value?
            # No, that breaks encapsulation.
            raise ValueError(
                f"Could not find linear unit for dimension {unit1.dimension(system)}"
            )

        return res_base, target_unit

    # 2. Linear - Offset -> Error? or Delta - T?
    # 5 deg - 20 degC = -15 degC (T) ?
    # Analogy: 5 steps - Position 20 = Position -15?
    # Usually: Delta - T is weird. T - Delta is OK.
    # Ref implementation: Quantity.py case 2 logic handled +/- similarly.
    # Let's support it if logic holds: res = base_linear - base_offset.
    # returns Offset Unit.
    if not is_off1 and is_off2:
        base1 = (
            xp.multiply(val1, getattr(conv1, "scale", 1.0)) if conv1 else val1
        )
        base2 = conv2.to_base(val2)
        res_base = xp.subtract(base1, base2)
        res_mag = conv2.from_base(res_base)
        return res_mag, unit2  # Result is T (unit2)

    # 3. Offset - Linear -> T (Offset)
    if is_off1 and not is_off2:
        base1 = conv1.to_base(val1)
        base2 = (
            xp.multiply(val2, getattr(conv2, "scale", 1.0)) if conv2 else val2
        )
        res_base = xp.subtract(base1, base2)
        res_mag = conv1.from_base(res_base)
        return res_mag, unit1

    # 4. Log
    is_log1 = isinstance(conv1, LogarithmicConverter)
    is_log2 = isinstance(conv2, LogarithmicConverter)
    if is_log1 and is_log2:
        base1 = conv1.to_base(val1)
        base2 = conv2.to_base(val2)
        res_base = xp.subtract(base1, base2)
        res_mag = conv1.from_base(res_base)
        return res_mag, unit1

    # Fallback
    factor = unit2.conversion_factor_to(unit1, system)
    val2_converted = xp.multiply(val2, factor)
    return xp.subtract(val1, val2_converted), unit1


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
