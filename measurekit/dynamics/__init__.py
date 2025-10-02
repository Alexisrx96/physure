"""The MeasureKit Dynamics package.

The `dynamics` package provides tools for solving problems in dynamics,
such as ordinary differential equations (ODEs). It integrates with the
`measurekit` library to provide a unit-aware solver that ensures dimensional
consistency in all calculations. This makes it ideal for simulations in
physics and engineering where dimensional correctness is crucial.
"""

from .solver import solve_unit_aware_ivp

__all__ = [
    "solve_unit_aware_ivp",
]
