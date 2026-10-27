"""Symbolic helpers follow the unit system that is active when they are created.

Their default used to be the system that was active when physure was first
imported, so a ``use_system`` block was silently ignored.
"""

import pytest

from physure.application.context import use_system
from physure.application.startup import create_system


@pytest.fixture
def imperial():
    return create_system("imperial.conf")


def test_symbolic_quantity_uses_the_active_system(imperial):
    from physure.domain.symbolic.quantity import SymbolicQuantity

    with use_system(imperial):
        x = SymbolicQuantity("x", "m")
    assert x.system is imperial


def test_symbolic_expression_uses_the_active_system(imperial):
    sp = pytest.importorskip("sympy")
    from physure.domain.measurement.units import CompoundUnit
    from physure.domain.symbolic.expression import SymbolicExpression

    with use_system(imperial):
        expression = SymbolicExpression(sp.Symbol("x"), CompoundUnit({"m": 1}))
    assert expression.system is imperial


def test_function_uses_the_active_system(imperial):
    sp = pytest.importorskip("sympy")
    from physure.application.functions.functions import Function
    from physure.domain.measurement.units import CompoundUnit

    meter = CompoundUnit({"m": 1})
    with use_system(imperial):
        function = Function(
            parameters={"x": meter},
            output_unit=meter,
            symbolic_func=sp.Symbol("x"),
        )
    assert function.system is imperial


def test_an_explicit_system_still_wins(imperial):
    from physure.domain.symbolic.quantity import SymbolicQuantity

    x = SymbolicQuantity("x", "m", system=imperial)
    assert x.system is imperial
