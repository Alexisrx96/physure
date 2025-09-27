import unittest

import sympy as sp

from measurekit import Q_, get_unit
from measurekit.functions.functions import Function
from measurekit.measurement.dimensions import Dimension


class TestFunction(unittest.TestCase):
    """Tests the Function class."""

    def test_function_initialization(self):
        """Tests the initialization of the Function class."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension.from_string("L")},
            output_dimension=Dimension.from_string("L") ** 2,
            symbolic_func=x**2,
        )
        self.assertEqual(func.arg_names, ("x",))
        self.assertEqual(str(func), "'x**2' with parameters {x: L}")

    def test_function_call(self):
        """Tests calling the function."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension({})},
            output_dimension=Dimension({}),
            symbolic_func=x * 2,  # type: ignore
        )
        self.assertEqual(func(get_unit("1"), x=Q_(5, "1")), Q_(10, "1"))

    def test_function_call_invalid_dimension(self):
        """Tests calling the function with a unit of the wrong dimension."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension.from_string("L")},
            output_dimension=Dimension.from_string("L"),
            symbolic_func=x,
        )
        with self.assertRaises(ValueError):
            # Attempting to call with seconds where meters are expected
            func(get_unit("m"), x=Q_(5, "s"))

    def test_function_call_invalid_output_dimension(self):
        """Tests calling the function with an invalid output unit dimension."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension.from_string("L")},
            output_dimension=Dimension.from_string("L"),  # Expects length
            symbolic_func=x,
        )
        with self.assertRaises(ValueError):
            # Attempting to get the output in seconds, which is a time dimension
            func(get_unit("s"), x=Q_(5, "m"))

    def test_function_call_missing_parameter(self):
        """Tests calling the function with a missing parameter."""
        x, y = sp.symbols("x y")
        func = Function(
            parameters={
                "x": Dimension.from_string("L"),
                "y": Dimension.from_string("T"),
            },
            output_dimension=Dimension.from_string("L*T"),
            symbolic_func=x * y,
        )
        with self.assertRaises(TypeError):
            func(get_unit("m*s"), x=Q_(5, "m"))

    def test_function_derivative(self):
        """Tests the derivative of the function."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension.from_string("L")},
            output_dimension=Dimension.from_string("L") ** 3,
            symbolic_func=x**3,
        )
        deriv = func.derivative(respect_to="x")
        self.assertEqual(deriv.symbolic_func, 3 * x**2)  # type: ignore
        self.assertEqual(deriv(get_unit("m**2"), x=Q_(4, "m")), Q_(48, "m**2"))

    def test_derivative_nonexistent_parameter(self):
        """Tests taking the derivative with respect to a nonexistent parameter."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension.from_string("L")},
            output_dimension=Dimension.from_string("L"),
            symbolic_func=x,
        )
        with self.assertRaises(ValueError):
            func.derivative(respect_to="y")

    def test_function_repr(self):
        """Tests the __repr__ method."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension({})},
            output_dimension=Dimension({}),
            symbolic_func=1 / x,  # type: ignore
        )
        self.assertEqual(
            repr(func),
            "Function(1/x, params={ x: Dimensionless }, output_dim=Dimensionless)",
        )

    def test_function_str(self):
        """Tests the __str__ method."""
        x = sp.Symbol("x")
        func = Function(
            parameters={"x": Dimension({})},
            output_dimension=Dimension({}),
            symbolic_func=x + 1,  # type: ignore
        )
        self.assertEqual(
            str(func), "'x + 1' with parameters {x: Dimensionless}"
        )


if __name__ == "__main__":
    unittest.main()
