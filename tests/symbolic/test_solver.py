import unittest

import sympy

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.symbolic.quantity import Equation, SymbolicQuantity
from tests.base_test_class import BaseTestUnit
from tests.decorators import with_system_context


class TestSymbolicQuantity(BaseTestUnit):
    """Tests for the SymbolicQuantity class."""

    def setUp(self):
        """Set up a fresh system for each test."""
        super().setUp()
        self.add_common_units()

    def test_initialization(self):
        """Test basic initialization."""
        mass = SymbolicQuantity("m", "kg", system=self.system)
        self.assertEqual(mass.symbol, sympy.Symbol("m", positive=True))

    @with_system_context
    def test_arithmetic_operations(self):
        """Test arithmetic operations between symbolic quantities."""
        m = SymbolicQuantity("m", "kg", system=self.system)
        a = SymbolicQuantity("a", "m/s^2", system=self.system)

        # Multiplication
        force = m * a
        # FIX: Use symbols with correct assumptions for comparison
        m_sym = sympy.Symbol("m", positive=True)
        a_sym = sympy.Symbol("a", positive=True)
        self.assertEqual(force.symbol, m_sym * a_sym)  # type: ignore
        self.assertEqual(force.unit, self.system.get_unit("kg*m/s^2"))

        # Division
        val = force / a
        self.assertEqual(val.symbol, (m_sym * a_sym) / a_sym)  # type: ignore
        self.assertEqual(val.unit, self.system.get_unit("kg"))

        # Power
        L_sym = sympy.Symbol("L", positive=True)
        area = SymbolicQuantity("L", "m", system=self.system) ** 2
        self.assertEqual(area.symbol, L_sym**2)
        self.assertEqual(area.unit, self.system.get_unit("m^2"))

    @with_system_context
    def test_operations_with_scalars(self):
        """Test operations with numeric scalars."""
        length = SymbolicQuantity("L", "m", system=self.system)
        L_sym = sympy.Symbol("L", positive=True)

        # Multiplication
        doubled = length * 2
        self.assertEqual(doubled.symbol, 2 * L_sym)  # type: ignore
        self.assertEqual(doubled.unit, self.system.get_unit("m"))

        doubled_rev = 2 * length
        self.assertEqual(doubled_rev.symbol, 2 * L_sym)  # type: ignore

        # Division
        halved = length / 2
        self.assertEqual(halved.symbol, L_sym / 2)  # type: ignore

        # Inverse
        inv = 1 / length
        self.assertEqual(inv.symbol, 1 / L_sym)  # type: ignore
        self.assertEqual(inv.unit, self.system.get_unit("1/m"))

    @with_system_context
    def test_addition_and_subtraction(self):
        """Test addition and subtraction with compatible units."""
        L1 = SymbolicQuantity("L1", "m", system=self.system)
        L2 = SymbolicQuantity("L2", "m", system=self.system)

        total = L1 + L2
        # FIX: Use symbols with correct assumptions
        L1_sym = sympy.Symbol("L1", positive=True)
        L2_sym = sympy.Symbol("L2", positive=True)
        self.assertEqual(total.symbol, L1_sym + L2_sym)  # type: ignore
        self.assertEqual(total.unit, self.system.get_unit("m"))

        # Test with incompatible units
        t = SymbolicQuantity("t", "s", system=self.system)
        with self.assertRaises(IncompatibleUnitsError):
            _ = L1 + t


class TestEquationSolver(BaseTestUnit):
    """Tests for the Equation class and its solver."""

    def setUp(self):
        super().setUp()
        self.add_common_units()

    @with_system_context
    def test_equation_creation_and_solving(self):
        """Test solving a simple physics equation F=ma."""
        F = SymbolicQuantity("F", "N", system=self.system)
        m = SymbolicQuantity("m", "kg", system=self.system)
        a = SymbolicQuantity("a", "m/s^2", system=self.system)

        newtons_law = Equation(F, m * a, variables=[F, m, a])
        self.assertEqual(
            newtons_law.equation,
            sympy.Eq(F.symbol, m.symbol * a.symbol),  # type: ignore
        )

        # Solve for a
        solution_a = newtons_law.solve_for("a")
        self.assertEqual(solution_a.symbol, F.symbol / m.symbol)  # type: ignore
        # Compare the exponent dictionaries for dimensional equivalence
        self.assertEqual(
            solution_a.unit.exponents,  # type: ignore
            self.system.get_unit("m/s^2").exponents,  # type: ignore
        )

        # Solve for m
        solution_m = newtons_law.solve_for(m)
        self.assertEqual(solution_m.symbol, F.symbol / a.symbol)  # type: ignore
        self.assertEqual(solution_m.unit, self.system.get_unit("kg"))  # type: ignore

    @with_system_context
    def test_kinematics_equation(self):
        """Test a more complex kinematics equation: d = v*t + 0.5*a*t^2."""
        d = SymbolicQuantity("d", "m", system=self.system)
        v = SymbolicQuantity("v", "m/s", system=self.system)
        t = SymbolicQuantity("t", "s", system=self.system)
        a = SymbolicQuantity("a", "m/s^2", system=self.system)

        term1 = v * t
        term2 = 0.5 * a * (t**2)
        rhs = term1 + term2

        kinematics_eq = Equation(d, rhs, variables=[d, v, t, a])

        solution_a = kinematics_eq.solve_for("a")
        expected_expr = 2 * (d.symbol - v.symbol * t.symbol) / (t.symbol**2)  # type: ignore

        self.assertEqual(sympy.simplify(solution_a.symbol - expected_expr), 0)  # type: ignore
        self.assertEqual(solution_a.unit, self.system.get_unit("m/s^2"))  # type: ignore

    @with_system_context
    def test_incompatible_equation(self):
        """Test creating an equation with incompatible sides."""
        F = SymbolicQuantity("F", "N", system=self.system)
        d = SymbolicQuantity("d", "m", system=self.system)

        with self.assertRaises(IncompatibleUnitsError):
            _ = Equation(F, d, variables=[F, d])

    @with_system_context
    def test_solving_with_dimensionless_constant(self):
        """Test equation with a dimensionless variable."""
        Re = SymbolicQuantity("Re", "1", system=self.system)
        rho = SymbolicQuantity("rho", "kg/m^3", system=self.system)
        v = SymbolicQuantity("v", "m/s", system=self.system)
        L = SymbolicQuantity("L", "m", system=self.system)
        mu = SymbolicQuantity("mu", "kg/(m*s)", system=self.system)

        reynolds_eq = Equation(
            Re, (rho * v * L) / mu, variables=[Re, rho, v, L, mu]
        )

        solution_mu = reynolds_eq.solve_for("mu")
        self.assertEqual(
            solution_mu.symbol,  # type: ignore
            (rho.symbol * v.symbol * L.symbol) / Re.symbol,  # type: ignore
        )
        self.assertEqual(solution_mu.unit, self.system.get_unit("kg/(m*s)"))  # type: ignore

    @with_system_context
    def test_no_solution(self):
        """Test an equation that has no solution for the given variable."""
        x = SymbolicQuantity("x", "m", system=self.system)
        y = SymbolicQuantity("y", "m", system=self.system)
        eq = Equation(5 * x / x, 10 * y / y, variables=[x, y])

        solution = eq.solve_for("x")
        self.assertIsNone(solution)


if __name__ == "__main__":
    unittest.main()
