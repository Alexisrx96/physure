"""Tests the Function class using pytest."""

import sympy as sp

from measurekit import Q_, get_unit
from measurekit.application.functions.functions import Function


def test_function_initialization():
    """Test standard Function initialization."""
    x = sp.Symbol("x")
    func = Function(
        parameters={"x": get_unit("m")},
        output_unit=get_unit("m**2"),
        symbolic_func=x**2,
    )
    assert func.arg_names == ("x",)


def test_function_call():
    """Test calling a Function object."""
    x = sp.Symbol("x")
    func = Function(
        parameters={"x": get_unit("1")},
        output_unit=get_unit("1"),
        symbolic_func=x * 2,
    )
    assert func(get_unit("1"), x=Q_(5, "1")) == Q_(10, "1")


def test_function_derivative():
    """Test symbolic derivation of a Function."""
    x = sp.Symbol("x")
    func = Function(
        parameters={"x": get_unit("m")},
        output_unit=get_unit("m**3"),
        symbolic_func=x**3,
    )
    deriv = func.derivative(respect_to="x")
    # Derivative of x^3 is 3x^2. Unit should be m^3 / m = m^2
    assert deriv(get_unit("m**2"), x=Q_(4, "m")) == Q_(48, "m**2")
