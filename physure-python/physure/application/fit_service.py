"""This module provides a unit-aware curve fitter.

It wraps `scipy.optimize.curve_fit`, letting users fit a model expressed in
`physure.Quantity` objects to experimental x/y data. Fitted parameters come
back as `Quantity` objects carrying the unit they were seeded with and the
standard-error uncertainty from the fit's covariance matrix.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

try:
    import numpy as np
    from scipy.optimize import curve_fit
except ImportError:
    np = None
    curve_fit = None

from physure.domain.measurement.quantity import Quantity

if TYPE_CHECKING:
    from collections.abc import Callable


class CurveFitResult:
    """Stores and presents the result of a unit-aware curve fit."""

    def __init__(self, params: list[Quantity], covariance: Any):
        """Initializes a CurveFitResult with fitted params and covariance."""
        self.params = params
        self.covariance = covariance

    def __repr__(self) -> str:
        """Provides a concise string representation of the fit result."""
        params_str = ", ".join(f"{p:.4g}" for p in self.params)
        return f"CurveFitResult(params=[{params_str}])"


def unit_aware_curve_fit(
    model: Callable[..., Quantity],
    xdata: Quantity,
    ydata: Quantity,
    p0: list[Quantity],
    **kwargs: Any,
) -> CurveFitResult:
    """Fits a unit-aware model to data using `scipy.optimize.curve_fit`.

    Args:
        model: A callable `model(x: Quantity, *params: Quantity) -> Quantity`.
        xdata: Independent variable data as a vectorized Quantity.
        ydata: Dependent variable data as a vectorized Quantity.
        p0: Initial guesses for each fit parameter, each a scalar Quantity
            whose unit determines the unit of the corresponding fitted param.
        **kwargs: Forwarded to `scipy.optimize.curve_fit`.

    Returns:
        A CurveFitResult with fitted `params` (as Quantities with standard-
        error uncertainty attached) and the raw covariance matrix.
    """
    if curve_fit is None:
        raise ImportError(
            "unit_aware_curve_fit requires scipy. Install it with "
            "'pip install scipy'."
        )

    # --- 1. Unit Unpacking (ONCE) ---
    sys = xdata.system
    x_unit = xdata.unit
    y_unit = ydata.unit
    p0_units = [p.unit for p in p0]
    p0_magnitudes = [p.magnitude for p in p0]

    # --- 2. Function Wrapper Creation ---
    def model_wrapper(x_arr: np.ndarray, *param_vals: float) -> np.ndarray:
        x_q = Quantity(x_arr, x_unit, system=sys)
        param_qs = [
            Quantity(val, unit, system=sys)
            for val, unit in zip(param_vals, p0_units, strict=False)
        ]
        result_q = model(x_q, *param_qs)
        return np.asarray(result_q.to(y_unit).magnitude)

    # ponytail: weight by ydata's own error bars when present, instead of
    # silently dropping them. absolute_sigma=True since these are physical
    # measurement uncertainties, not relative weights.
    y_std_dev = ydata.uncertainty
    if np.any(np.asarray(y_std_dev) != 0):
        kwargs.setdefault("sigma", y_std_dev)
        kwargs.setdefault("absolute_sigma", True)

    # --- 3. Calling SciPy Optimizer ---
    popt, pcov = curve_fit(
        model_wrapper,
        xdata.magnitude,
        ydata.magnitude,
        p0=p0_magnitudes,
        **kwargs,
    )

    # --- 4. Repackaging Fitted Parameters (ONCE) ---
    std_errors = np.sqrt(np.diag(pcov))
    params = [
        Quantity.from_input(
            popt[i], p0_units[i], sys, uncertainty=std_errors[i]
        )
        for i in range(len(popt))
    ]

    return CurveFitResult(params, pcov)
