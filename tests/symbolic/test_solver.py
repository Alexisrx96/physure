"""Tests for symbolic quantities and equation solving using pytest."""

import pytest
import sympy

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.symbolic.quantity import Equation, SymbolicQuantity


@pytest.fixture
def symbolic_system(common_system):
    """Provides a UnitSystem for symbolic tests."""
    return common_system


def test_symbolic_quantity_initialization(symbolic_system):
    """Test basic initialization of SymbolicQuantity."""
    mass = SymbolicQuantity("m", "kg", system=symbolic_system)
    assert mass.symbol == sympy.Symbol("m", positive=True)


def test_symbolic_quantity_arithmetic_operations(symbolic_system):
    """Test arithmetic operations between symbolic quantities."""
    m = SymbolicQuantity("m", "kg", system=symbolic_system)
    a = SymbolicQuantity("a", "m/s^2", system=symbolic_system)

    # Multiplication
    force = m * a
    m_sym = sympy.Symbol("m", positive=True)
    a_sym = sympy.Symbol("a", positive=True)

    assert force.expr == m_sym * a_sym
    assert force.unit == symbolic_system.get_unit("kg*m/s^2")

    # Division
    val = force / a
    assert val.expr == (m_sym * a_sym) / a_sym
    assert val.unit == symbolic_system.get_unit("kg")

    # Power
    l_sym = sympy.Symbol("L", positive=True)
    area = SymbolicQuantity("L", "m", system=symbolic_system) ** 2
    assert area.expr == l_sym**2
    assert area.unit == symbolic_system.get_unit("m^2")


def test_symbolic_quantity_operations_with_scalars(symbolic_system):
    """Test operations with numeric scalars."""
    length = SymbolicQuantity("L", "m", system=symbolic_system)
    l_sym = sympy.Symbol("L", positive=True)

    # Multiplication
    doubled = length * 2
    assert doubled.expr == 2 * l_sym
    assert doubled.unit == symbolic_system.get_unit("m")

    doubled_rev = 2 * length
    assert doubled_rev.expr == 2 * l_sym

    # Division
    halved = length / 2
    assert halved.expr == l_sym / 2

    # Inverse
    inv = 1 / length
    assert inv.expr == 1 / l_sym
    assert inv.unit == symbolic_system.get_unit("1/m")


def test_symbolic_quantity_addition_and_subtraction(symbolic_system):
    """Test addition and subtraction with compatible units."""
    l1 = SymbolicQuantity("L1", "m", system=symbolic_system)
    l2 = SymbolicQuantity("L2", "m", system=symbolic_system)

    total = l1 + l2
    l1_sym = sympy.Symbol("L1", positive=True)
    l2_sym = sympy.Symbol("L2", positive=True)
    assert total.expr == l1_sym + l2_sym
    assert total.unit == symbolic_system.get_unit("m")

    # Test with incompatible units
    t = SymbolicQuantity("t", "s", system=symbolic_system)
    with pytest.raises(IncompatibleUnitsError):
        _ = l1 + t


def test_equation_creation_and_solving(symbolic_system):
    """Test solving a simple physics equation F=ma."""
    f = SymbolicQuantity("F", "N", system=symbolic_system)
    m = SymbolicQuantity("m", "kg", system=symbolic_system)
    a = SymbolicQuantity("a", "m/s^2", system=symbolic_system)

    newtons_law = Equation(f, m * a, variables=[f, m, a])
    assert newtons_law.equation == sympy.Eq(f.symbol, m.symbol * a.symbol)

    # Solve for a
    solution_a = newtons_law.solve_for(a)
    assert solution_a.expr == f.symbol / m.symbol
    assert (
        solution_a.unit.exponents
        == symbolic_system.get_unit("m/s^2").exponents
    )

    # Solve for m
    solution_m = newtons_law.solve_for(m)
    assert solution_m.expr == f.symbol / a.symbol
    assert solution_m.unit == symbolic_system.get_unit("kg")


def test_kinematics_equation(symbolic_system):
    """Test a more complex kinematics equation: d = v*t + 0.5*a*t^2."""
    d = SymbolicQuantity("d", "m", system=symbolic_system)
    v = SymbolicQuantity("v", "m/s", system=symbolic_system)
    t = SymbolicQuantity("t", "s", system=symbolic_system)
    a = SymbolicQuantity("a", "m/s^2", system=symbolic_system)

    rhs = v * t + 0.5 * a * (t**2)
    kinematics_eq = Equation(d, rhs, variables=[d, v, t, a])

    solution_a = kinematics_eq.solve_for("a")
    expected_expr = 2 * (d.symbol - v.symbol * t.symbol) / (t.symbol**2)

    assert sympy.simplify(solution_a.expr - expected_expr) == 0
    assert solution_a.unit == symbolic_system.get_unit("m/s^2")


def test_incompatible_equation(symbolic_system):
    """Test creating an equation with incompatible sides."""
    f = SymbolicQuantity("F", "N", system=symbolic_system)
    d = SymbolicQuantity("d", "m", system=symbolic_system)

    with pytest.raises(IncompatibleUnitsError):
        _ = Equation(f, d, variables=[f, d])


def test_solving_with_dimensionless_constant(symbolic_system):
    """Test equation with a dimensionless variable."""
    re = SymbolicQuantity("re", "1", system=symbolic_system)
    rho = SymbolicQuantity("rho", "kg/m^3", system=symbolic_system)
    v = SymbolicQuantity("v", "m/s", system=symbolic_system)
    le = SymbolicQuantity("l", "m", system=symbolic_system)
    mu = SymbolicQuantity("mu", "kg/(m*s)", system=symbolic_system)

    reynolds_eq = Equation(
        re, (rho * v * le) / mu, variables=[re, rho, v, le, mu]
    )

    solution_mu = reynolds_eq.solve_for("mu")
    assert solution_mu.expr == (rho.symbol * v.symbol * le.symbol) / re.symbol
    assert solution_mu.unit == symbolic_system.get_unit("kg/(m*s)")


def test_no_solution(symbolic_system):
    """Test an equation that has no solution for the given variable."""
    x = SymbolicQuantity("x", "m", system=symbolic_system)
    y = SymbolicQuantity("y", "m", system=symbolic_system)
    eq = Equation(5 * x / x, 10 * y / y, variables=[x, y])

    solution = eq.solve_for("x")
    assert solution is None
