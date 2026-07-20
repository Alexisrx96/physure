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


def linspace(start: Any, stop: Any, num: int = 50) -> Any:
    """Generate linearly spaced numbers or Quantities."""
    import numpy as np

    start_val = getattr(start, "magnitude", start)
    stop_val = getattr(stop, "magnitude", stop)
    unit = getattr(start, "unit", getattr(stop, "unit", None))
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
