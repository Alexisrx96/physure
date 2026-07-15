"""Tests for the ODE solver using pytest."""

import math

try:
    import numpy as np
except (ImportError, ModuleNotFoundError):
    np = None

import pytest

try:
    from physure.application.solver_service import (
        ODESolution,
        solve_unit_aware_ivp,
    )
except (ImportError, AttributeError):
    # Solver service might fail if numpy/scipy missing
    ODESolution = None
    solve_unit_aware_ivp = None
from physure.domain.measurement.converters import LinearConverter
from physure.domain.measurement.dimensions import Dimension


@pytest.fixture
def solver_system(system):
    """Set up test fixtures for solver tests."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("g", mass, LinearConverter(0.001), "gram")
    return system


@pytest.mark.skipif(ODESolution is None, reason="solver dependencies missing")
def test_odesolution_init_and_repr(solver_system):
    """Test the initialization and representation for ODESolution."""
    t_quantities = solver_system.Q_(np.linspace(0, 1, 5), "s")
    y_quantities = [solver_system.Q_(np.linspace(0, 10, 5), "m")]

    sol = ODESolution(t=t_quantities, y=y_quantities)

    assert len(sol.t) == 5
    assert sol.t[0].magnitude == 0.0
    assert sol.t.unit == solver_system.get_unit("s")
    assert len(sol.y) == 1
    assert len(sol.y[0]) == 5
    assert sol.y[0].magnitude[0] == 0.0
    assert sol.y[0].unit == solver_system.get_unit("m")

    expected_repr = (
        f"ODESolution(t=[{sol.t[0]:.2f}...{sol.t[-1]:.2f}],"
        f" num_states={len(sol.y)})"
    )
    assert repr(sol) == expected_repr


@pytest.mark.skipif(
    solve_unit_aware_ivp is None, reason="solver dependencies missing"
)
def test_solve_unit_aware_ivp_simple_decay(solver_system):
    """Test with a simple first-order ODE: dy/dt = -k*y."""
    k = solver_system.Q_(0.1, "1/s")

    def decay_rate(t, y):
        return [-k * y[0]]

    y0 = [solver_system.Q_(100.0, "g")]
    t_span = [solver_system.Q_(0.0, "s"), solver_system.Q_(10.0, "s")]

    sol = solve_unit_aware_ivp(decay_rate, t_span, y0)

    assert isinstance(sol, ODESolution)
    assert sol.t.unit == solver_system.get_unit("s")
    assert sol.y[0].unit == solver_system.get_unit("g")
    assert np.isclose(sol.t[0].magnitude, 0.0)
    assert np.isclose(sol.y[0].magnitude[0], 100.0)

    final_t = sol.t[-1].magnitude
    final_y_actual = sol.y[0].magnitude[-1]
    final_y_expected = 100.0 * math.exp(-0.1 * final_t)
    assert np.isclose(final_y_actual, final_y_expected, rtol=1e-2)
