"""Applies the conversion plan that the core registry gives for two units.

The core decides how a number in one unit becomes a number in another: a
factor, two temperature scales with their zero points, or a level (decibel)
through the ratio it stands for. The numbers of a `Quantity` live in NumPy,
torch or JAX, so the plan is applied here with the operations of the backend
of the quantity. Nothing is decided in this module: the plan says what to do.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
)

if TYPE_CHECKING:
    from physure.core.protocols import BackendOps


def _stage(stage: dict[str, float]) -> LinearConverter | LogarithmicConverter:
    """One side of a conversion through a ratio, as the converter of that side."""
    if "scale" in stage:
        return LinearConverter(stage["scale"])
    return LogarithmicConverter(stage["factor"], stage["reference"])


def apply_conversion(
    plan: dict[str, Any],
    magnitude: Any,
    uncertainty: Any,
    backend: BackendOps,
) -> tuple[Any, Any]:
    """Converts a magnitude and its uncertainty by the plan of the core.

    Args:
        plan: What `UnitRegistry.conversion` returned: an affine plan
            (``new = old * scale + shift``) or a level plan (the two stages of
            a conversion through the ratio both units stand for).
        magnitude: The value to convert.
        uncertainty: Its standard uncertainty.
        backend: The backend operations of the quantity.

    Returns:
        The converted magnitude and uncertainty. The uncertainty follows the
        chain rule: the slope of the plan at the magnitude.
    """
    if plan["kind"] == "affine":
        scale, shift = plan["scale"], plan["shift"]
        new_magnitude = backend.mul(magnitude, scale)
        if shift != 0.0:
            new_magnitude = backend.add(new_magnitude, shift)
        return new_magnitude, backend.mul(uncertainty, abs(scale))

    source, target = _stage(plan["from"]), _stage(plan["to"])
    ratio = source.to_base(magnitude)
    new_magnitude = target.from_base(ratio)
    slope = backend.mul(
        source.to_base_derivative(magnitude),
        target.from_base_derivative(ratio),
    )
    return new_magnitude, backend.mul(uncertainty, backend.abs(slope))
