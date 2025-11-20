import unittest
import sympy as sp
from measurekit import Q_, get_unit
from measurekit.application.functions.functions import Function


class TestFunction(unittest.TestCase):
    """Tests the Function class."""

    def test_function_initialization(self):
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": get_unit("m")},
            output_unit=get_unit("m**2"),
            symbolic_func=x**2,
        )
        self.assertEqual(func.arg_names, ("x",))

    def test_function_call(self):
        x = sp.Symbol("x")
        # "1" is dimensionless in the updated config
        func = Function(
            parameters={"x": get_unit("1")},
            output_unit=get_unit("1"),
            symbolic_func=x * 2,
        )
        self.assertEqual(func(get_unit("1"), x=Q_(5, "1")), Q_(10, "1"))

    def test_function_derivative(self):
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": get_unit("m")},
            output_unit=get_unit("m**3"),
            symbolic_func=x**3,
        )
        deriv = func.derivative(respect_to="x")
        # Derivative of x^3 is 3x^2. Unit should be m^3 / m = m^2
        self.assertEqual(deriv(get_unit("m**2"), x=Q_(4, "m")), Q_(48, "m**2"))


if __name__ == "__main__":
    unittest.main()
