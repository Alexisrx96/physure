"""Helper functions for exported Physure DSL scripts."""

from __future__ import annotations

import math
from typing import Any

from physure.application.factories import QuantityFactory

Q_ = QuantityFactory()

pi = math.pi
e = math.e


def _to_float(val: Any) -> float:
    return float(getattr(val, "magnitude", val))


def approx_eq(
    a: Any, b: Any, rel_tol: float = 0.1, abs_tol: float = 1e-5
) -> bool:
    """Check approximate equality between numbers or Quantities."""
    if hasattr(a, "approx_eq") and hasattr(b, "_core") and hasattr(a, "_core"):
        try:
            return bool(a._core.approx_eq(b._core, rel_tol, abs_tol))
        except Exception:
            pass
    a_mag = getattr(a, "magnitude", a)
    b_mag = getattr(b, "magnitude", b)
    if hasattr(a, "to") and hasattr(b, "unit"):
        try:
            b_mag = b.to(a.unit).magnitude
        except Exception:
            return False
    try:
        return math.isclose(
            float(a_mag), float(b_mag), rel_tol=rel_tol, abs_tol=abs_tol
        )
    except Exception:
        return False


def _core_quantity(x: Any) -> Any:
    """The Rust core's quantity for a number or a Quantity, its unit read by the registry."""
    from physure._core import Quantity as _CoreQuantity
    from physure.domain.measurement.units import get_default_system

    if isinstance(x, (int, float)) and not isinstance(x, bool):
        x = Q_(float(x), "")
    unit = get_default_system().core_registry.expand(x.unit)
    return _CoreQuantity(float(x.magnitude), unit)


def phs_compare(a: Any, op: str, b: Any) -> bool:
    """PHS `a op b` (`op` one of `<`, `>`, `<=`, `>=`, `==`, `!=`), as the interpreter answers it.

    Two numbers or quantities are compared by the Rust core (`Quantity::phs_compare`):
    canonical magnitudes, `==` within the relative tolerance of PHS, and a `ValueError` when
    they have no comparison (different dimensions, a zero written without a unit, a level
    against a plain number). Two Bools or two strings compare with `==` and `!=` only; any
    other pair is a `TypeError`. This is what PHS transpiled to Python calls.
    """
    for kind in (bool, str):
        if isinstance(a, kind) or isinstance(b, kind):
            if (
                isinstance(a, kind)
                and isinstance(b, kind)
                and op in ("==", "!=")
            ):
                return (a == b) == (op == "==")
            raise TypeError(
                f"cannot compare {type(a).__name__} and {type(b).__name__} with {op}"
            )
    return bool(_core_quantity(a).phs_compare(_core_quantity(b), op))


def dof(x: Any) -> float:
    """Effective degrees of freedom of a measurement (`inf` for a number or an exact quantity)."""
    return float(getattr(x, "dof", math.inf))


def _probability(p: Any) -> float:
    """A coverage probability: a number, or the dimensionless quantity a PHS literal becomes."""
    return float(getattr(p, "magnitude", p))


def coverage_factor(x: Any, p: Any) -> float:
    """Coverage factor for the coverage probability `p` (Student-t at the effective dof)."""
    from physure._core import coverage_factor as _coverage_factor

    return float(_coverage_factor(_probability(p), dof(x)))


def expanded(x: Any, p: Any) -> Any:
    """Expanded uncertainty `U = k u_c`, an exact quantity in the unit of `x` (0.0 for a number)."""
    if not hasattr(x, "expanded"):
        return 0.0
    result = x.expanded(_probability(p))
    return Q_(result.expanded_uncertainty, result.unit)


def from_samples(values: Any) -> Any:
    """Type A evaluation of repeated observations (mean, standard error, `n - 1` dof)."""
    return Q_.from_samples(values)


def linspace(start: Any, stop: Any, num: int = 50) -> Any:
    """Generate linearly spaced numbers or Quantities, in the unit of the first quantity.

    As PHS `linspace`: each endpoint is converted to that unit (`1 km` is 1000 in metres),
    so endpoints of different dimensions, or a plain number with a quantity that has a
    dimension, raise ``IncompatibleUnitsError``; an endpoint that is neither a number nor
    a quantity raises ``TypeError``.
    """
    import numpy as np

    for end in (start, stop):
        if not hasattr(end, "magnitude") and (
            isinstance(end, bool) or not isinstance(end, (int, float))
        ):
            raise TypeError(
                f"linspace's endpoints are numbers or quantities, got {end!r}"
            )
    unit = getattr(start, "unit", getattr(stop, "unit", None))
    if unit is None:
        start_val, stop_val = start, stop
    else:
        quantities = (
            e if hasattr(e, "magnitude") else Q_(e, "") for e in (start, stop)
        )
        start_val, stop_val = (q.to(unit).magnitude for q in quantities)
    arr = np.linspace(float(start_val), float(stop_val), int(num))
    if unit:
        return Q_(arr, str(unit))
    return arr


def plot(*args: Any) -> None:
    """Plot physical quantities using matplotlib."""
    try:
        import matplotlib.pyplot as plt

        _fig, ax = plt.subplots()
        if len(args) >= 2:
            x, y = args[0], args[1]
            x_val = getattr(x, "magnitude", x)
            y_val = getattr(y, "magnitude", y)
            ax.plot(x_val, y_val)
        elif len(args) == 1:
            y = args[0]
            y_val = getattr(y, "magnitude", y)
            ax.plot(y_val)
        plt.show()
    except Exception as err:
        print(f"Plot error: {err}")


def sqrt(x: Any) -> Any:
    """Square root supporting Quantities and numbers."""
    if hasattr(x, "unit") or hasattr(x, "magnitude"):
        return x**0.5
    return math.sqrt(float(x))


def sin(x: Any) -> float:
    """Sine of scalar value."""
    return math.sin(_to_float(x))


def cos(x: Any) -> float:
    """Cosine of scalar value."""
    return math.cos(_to_float(x))


def tan(x: Any) -> float:
    """Tangent of scalar value."""
    return math.tan(_to_float(x))


def asin(x: Any) -> float:
    """Arcsine of scalar value."""
    return math.asin(_to_float(x))


def acos(x: Any) -> float:
    """Arccosine of scalar value."""
    return math.acos(_to_float(x))


def atan(x: Any) -> float:
    """Arctangent of scalar value."""
    return math.atan(_to_float(x))


def atan2(y: Any, x: Any) -> float:
    """Two-argument arctangent of scalar values."""
    return math.atan2(_to_float(y), _to_float(x))


def sinh(x: Any) -> float:
    """Hyperbolic sine of scalar value."""
    return math.sinh(_to_float(x))


def cosh(x: Any) -> float:
    """Hyperbolic cosine of scalar value."""
    return math.cosh(_to_float(x))


def tanh(x: Any) -> float:
    """Hyperbolic tangent of scalar value."""
    return math.tanh(_to_float(x))


def exp(x: Any) -> float:
    """Exponential of scalar value."""
    return math.exp(_to_float(x))


def log(x: Any) -> float:
    """Natural logarithm of scalar value."""
    return math.log(_to_float(x))


def log10(x: Any) -> float:
    """Base-10 logarithm of scalar value."""
    return math.log10(_to_float(x))
