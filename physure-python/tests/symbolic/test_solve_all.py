"""Tests for Equation.solve_all (multi-root solving)."""

import pytest
import sympy

from physure.domain.symbolic.quantity import Equation, SymbolicQuantity


@pytest.fixture
def symbolic_system(common_system):
    """Provides a UnitSystem for symbolic tests."""
    return common_system


def test_solve_all_returns_every_root(symbolic_system):
    """(x - a)(x - b) = 0 has two roots; both must be exposed."""
    x = SymbolicQuantity("x", "m", system=symbolic_system)
    a = SymbolicQuantity("a", "m", system=symbolic_system)
    b = SymbolicQuantity("b", "m", system=symbolic_system)

    lhs = (x - a) * (x - b)
    rhs = 0 * a * b
    eq = Equation(lhs, rhs, variables=[x, a, b])

    solutions = eq.solve_all("x")
    exprs = {sol.expr for sol in solutions}

    assert exprs == {
        sympy.Symbol("a", positive=True),
        sympy.Symbol("b", positive=True),
    }
    for sol in solutions:
        assert sol.unit == symbolic_system.get_unit("m")


def test_solve_for_still_returns_single_root(symbolic_system):
    x = SymbolicQuantity("x", "m", system=symbolic_system)
    a = SymbolicQuantity("a", "m", system=symbolic_system)
    b = SymbolicQuantity("b", "m", system=symbolic_system)

    eq = Equation((x - a) * (x - b), 0 * a * b, variables=[x, a, b])
    sol = eq.solve_for("x")

    assert sol is not None
    assert sol.expr in eq.sympy_eq.free_symbols or sol.expr in {
        sympy.Symbol("a", positive=True),
        sympy.Symbol("b", positive=True),
    }
