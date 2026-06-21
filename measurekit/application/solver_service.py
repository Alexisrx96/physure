# measurekit/dynamics/solver.py
"""This module provides a unit-aware solver for ordinary differential equation.

It wraps the powerful `solve_ivp` function from the SciPy library, allowing
users to define their differential equations using `measurekit.Quantity`
objects. This ensures that all calculations are dimensionally correct,
preventing common errors in physics and engineering simulations.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import TYPE_CHECKING

try:
    import numpy as np
    from scipy.integrate import solve_ivp
except ImportError:
    np = None
    solve_ivp = None

from measurekit.domain.measurement.quantity import Quantity

if TYPE_CHECKING:
    from numpy.typing import NDArray


class ODESolution:
    """A class to store and present the solution of an ODE.

    Allows for easy access to the results.
    """

    def __init__(self, t: Quantity, y: list[Quantity]):
        """Initializes the ODESolution with time points and state values."""
        self.t = t
        self.y = y

    def __repr__(self):
        """Provides a concise string representation of the solution."""
        # Check if the solution contains any time steps
        if self.t.magnitude.size == 0:
            return f"ODESolution(t=[], num_states={len(self.y)})"

        # Create scalar Quantity objects for the start and end times
        # This allows us to use Quantity's own formatting, which includes
        # units.
        t_start = self.t[0]
        t_end = self.t[-1]

        return (
            f"ODESolution(t=[{t_start:.2f}...{t_end:.2f}],"
            f" num_states={len(self.y)})"
        )


def solve_unit_aware_ivp(
    fun: Callable[[Quantity, list[Quantity]], list[Quantity]],
    t_span: list[Quantity],
    y0: list[Quantity],
    t_eval: NDArray[np.floating] | None = None,
    **kwargs,
) -> ODESolution:
    """Solves an initial value problem.

    This function is a wrapper around `scipy.integrate.solve_ivp`, allowing
    users to define their differential equations using `measurekit.Quantity`
    objects. It ensures that all calculations are dimensionally consistent.
    """
    # --- 1. Unit Unpacking (ONCE) ---
    t_unit = t_span[0].unit
    y0_magnitudes = np.array([q.magnitude for q in y0])
    y0_units = [q.unit for q in y0]

    dydt_units = [state_unit / t_unit for state_unit in y0_units]
    t_span_magnitudes = [t_span[0].magnitude, t_span[1].to(t_unit).magnitude]

    # --- 2. Function Wrapper Creation ---
    def fun_wrapper(t_val: float, y_vals: np.ndarray) -> np.ndarray:
        # a. Repackage into Quantities (context-aware)
        t_q = Quantity(t_val, t_unit)
        y_q = [
            Quantity(val, unit)
            for val, unit in zip(y_vals, y0_units, strict=False)
        ]

        # b. Call the user's original function
        dy_dt_q = fun(t_q, y_q)

        # c. Unpack derivatives into a numeric array, converting units
        dy_dt_magnitudes = np.array(
            [
                res.to(expected_unit).magnitude
                for res, expected_unit in zip(
                    dy_dt_q, dydt_units, strict=False
                )
            ]
        )
        return dy_dt_magnitudes

    # --- 3. Calling the SciPy Solver ---
    sol = solve_ivp(
        fun_wrapper, t_span_magnitudes, y0_magnitudes, t_eval=t_eval, **kwargs
    )

    # --- 4. Repackaging the Final Solution (ONCE) ---
    sys = t_span[0].system
    solution_t = Quantity(sol.t, t_unit, system=sys)
    solution_y = [
        Quantity(state_values, y0_units[i], system=sys)
        for i, state_values in enumerate(sol.y)
    ]

    return ODESolution(solution_t, solution_y)
