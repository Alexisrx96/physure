import math
import unittest

import numpy as np

from measurekit.dynamics.solver import ODESolution, solve_unit_aware_ivp
from measurekit.measurement.dimensions import Dimension
from tests.base_test_class import BaseTestUnit


class TestODESolution(BaseTestUnit):
    """Tests for the ODESolution class."""

    def setUp(self):
        """Set up a fresh system with necessary units for each test."""
        super().setUp()
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        self.system.register_unit("m", length, 1.0, "meter")
        self.system.register_unit("s", time, 1.0, "second")

    def test_init_and_repr(self):
        """Test the initialization and representation of the ODESolution class."""
        t_quantities = self.system.Q_(np.linspace(0, 1, 5), "s")
        # The solver returns a list of Quantities, where each Quantity holds an array of values
        y_quantities = [self.system.Q_(np.linspace(0, 10, 5), "m")]

        sol = ODESolution(t=t_quantities, y=y_quantities)

        # Test __init__
        self.assertEqual(len(sol.t), 5)
        self.assertEqual(sol.t[0].magnitude, 0.0)
        self.assertEqual(sol.t.unit, self.system.get_unit("s"))
        self.assertEqual(len(sol.y), 1)
        self.assertEqual(len(sol.y[0]), 5)
        self.assertEqual(sol.y[0].magnitude[0], 0.0)
        self.assertEqual(sol.y[0].unit, self.system.get_unit("m"))

        # Test __repr__ by reconstructing the expected string
        expected_repr = (
            f"ODESolution(t=[{sol.t[0]:.2f}...{sol.t[-1]:.2f}],"
            f" num_states={len(sol.y)})"
        )
        self.assertEqual(repr(sol), expected_repr)


class TestSolveUnitAwareIvp(BaseTestUnit):
    """Tests for the solve_unit_aware_ivp function."""

    def setUp(self):
        """Set up a fresh system with necessary units for each test."""
        super().setUp()
        self.mass = Dimension({"M": 1})
        self.time = Dimension({"T": 1})
        self.system.register_unit("g", self.mass, 0.001, "gram")
        self.system.register_unit("s", self.time, 1.0, "second")

    def test_solve_unit_aware_ivp_simple_decay(self):
        """Test with a simple first-order ODE: dy/dt = -k*y."""
        k = self.system.Q_(0.1, "1/s")

        def decay_rate(t, y):
            return [-k * y[0]]

        y0 = [self.system.Q_(100.0, "g")]
        t_span = [self.system.Q_(0.0, "s"), self.system.Q_(10.0, "s")]

        sol = solve_unit_aware_ivp(decay_rate, t_span, y0)

        self.assertIsInstance(sol, ODESolution)
        self.assertEqual(sol.t.unit, self.system.get_unit("s"))
        self.assertEqual(sol.y[0].unit, self.system.get_unit("g"))
        self.assertAlmostEqual(sol.t[0].magnitude, 0.0)
        self.assertAlmostEqual(sol.y[0].magnitude[0], 100.0)

        final_t = sol.t[-1].magnitude
        final_y_actual = sol.y[0].magnitude[-1]
        final_y_expected = 100.0 * math.exp(-0.1 * final_t)
        self.assertAlmostEqual(final_y_actual, final_y_expected, places=1)


if __name__ == "__main__":
    unittest.main()
