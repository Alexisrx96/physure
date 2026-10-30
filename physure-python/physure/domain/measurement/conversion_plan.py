"""Applies the conversion plan that the core registry gives for two units.

The core decides how a number in one unit becomes a number in another: a
factor, two temperature scales with their zero points, or a level (decibel)
through the ratio it stands for. The numbers of a `Quantity` live in NumPy,
torch or JAX, so the plan is applied here with the operations of the backend
of the quantity. Nothing is decided in this module: the plan says what to do.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from physure.core.protocols import BackendOps

_LN10 = math.log(10.0)

# A stage is one side of a conversion through the ratio both units stand for:
# ``{"scale": s}`` (ratio = value * s) or ``{"factor": f, "reference": r}``
# (ratio = r * 10 ** (value / f), the level `f * log10(ratio / r)`).


def _to_ratio(stage: dict[str, float], value: Any) -> Any:
    """The ratio a value of this side stands for."""
    if "scale" in stage:
        return value * stage["scale"]
    return stage["reference"] * (10 ** (value / stage["factor"]))


def _from_ratio(stage: dict[str, float], ratio: Any) -> Any:
    """The value of this side for a ratio."""
    if "scale" in stage:
        return ratio / stage["scale"]
    # ponytail: the math path for a scalar keeps numpy an optional extra;
    # an array is numpy's.
    if isinstance(ratio, (int, float)):
        return stage["factor"] * math.log10(ratio / stage["reference"])
    import numpy as np

    return stage["factor"] * np.log10(ratio / stage["reference"])


def _slope_to_ratio(stage: dict[str, float], value: Any) -> Any:
    """d(ratio)/d(value) at a value of this side."""
    if "scale" in stage:
        return stage["scale"]
    return _to_ratio(stage, value) * _LN10 / stage["factor"]


def _slope_from_ratio(stage: dict[str, float], ratio: Any) -> Any:
    """d(value)/d(ratio) at a ratio."""
    if "scale" in stage:
        return 1.0 / stage["scale"]
    return stage["factor"] / (ratio * _LN10)


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

    source, target = plan["from"], plan["to"]
    ratio = _to_ratio(source, magnitude)
    new_magnitude = _from_ratio(target, ratio)
    slope = backend.mul(
        _slope_to_ratio(source, magnitude),
        _slope_from_ratio(target, ratio),
    )
    return new_magnitude, backend.mul(uncertainty, backend.abs(slope))
