"""Test suite for the Quantity class after refactoring."""

import unittest

import numpy as np

from measurekit.exceptions import IncompatibleUnitsError
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.units import CompoundUnit
from tests.base_test_class import BaseTestUnit


class TestQuantity(BaseTestUnit):
    """Tests for the Quantity class with an isolated UnitSystem."""

    def setUp(self):
        """Set up a fresh UnitSystem for each test."""
        super().setUp()

        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})

        self.system.register_unit("m", length, 1.0, "meter")
        self.system.register_unit("cm", length, 0.01, "centimeter")
        self.system.register_unit("km", length, 1000.0, "kilometer")
        self.system.register_unit("s", time, 1.0, "second")
        self.system.register_unit("min", time, 60.0, "minute")
        self.system.register_unit("h", time, 3600.0, "hour")
        self.system.register_unit("kg", mass, 1.0, "kilogram")
        self.system.register_unit("g", mass, 0.001, "gram")
        self.system.register_unit("rad", Dimension({}), 1.0, "radian")

        self.meter = CompoundUnit({"m": 1})
        self.centimeter = CompoundUnit({"cm": 1})
        self.kilometer = CompoundUnit({"km": 1})
        self.second = CompoundUnit({"s": 1})
        self.gram = CompoundUnit({"g": 1})

    def test_initialization(self):
        """Test basic initialization patterns."""
        q1 = self.system.Q_(5.0, self.meter)
        self.assertEqual(q1.magnitude, 5.0)
        self.assertEqual(q1.unit, self.meter)
        self.assertEqual(q1.dimension, self.meter.dimension(self.system))
        self.assertIs(q1.system, self.system)

    def test_conversion(self):
        """Test unit conversion with the to method."""
        length = self.system.Q_(5.0, self.meter)
        length_cm = length.to(self.centimeter)
        self.assertEqual(length_cm.magnitude, 500.0)
        self.assertEqual(length_cm.unit, self.centimeter)
        length_km_str = length.to("km")
        self.assertAlmostEqual(length_km_str.magnitude, 0.005)
        self.assertEqual(length_km_str.unit, self.kilometer)

    def test_arithmetic_operations(self):
        """Test arithmetic operations between quantities."""
        length1 = self.system.Q_(5.0, self.meter)
        length2 = self.system.Q_(10.0, self.meter)
        time = self.system.Q_(2.0, self.second)

        sum_length = length1 + length2
        self.assertEqual(sum_length.magnitude, 15.0)
        self.assertEqual(sum_length.unit, self.meter)

        diff_length = length2 - length1
        self.assertEqual(diff_length.magnitude, 5.0)

        double_length = length1 * 2
        self.assertEqual(double_length.magnitude, 10.0)

        velocity = length1 / time
        self.assertEqual(velocity.magnitude, 2.5)
        self.assertEqual(velocity.unit.exponents, {"m": 1, "s": -1})

    def test_comparison_operations(self):
        """Test comparison operations between quantities."""
        length1 = self.system.Q_(5.0, "m")
        length2 = self.system.Q_(500.0, "cm")
        length3 = self.system.Q_(10.0, "m")

        self.assertEqual(length1, length2)
        self.assertNotEqual(length1, length3)
        self.assertLess(length1, length3)
        self.assertGreater(length3, length1)

    def test_uncertainty_propagation(self):
        """Test the propagation of uncertainty for basic arithmetic."""
        q1 = self.system.Q_(10.0, self.meter, uncertainty=0.1)
        q2 = self.system.Q_(5.0, self.meter, uncertainty=0.2)

        result_add = q1 + q2
        self.assertAlmostEqual(result_add.magnitude, 15.0)
        self.assertAlmostEqual(result_add.uncertainty, 0.22361, places=5)

    def test_rtruediv_uncertainty(self):
        """Test uncertainty for inverse division (1/q)."""
        q = self.system.Q_(4.0, self.meter, uncertainty=0.1)
        result = 1 / q

        self.assertAlmostEqual(result.magnitude, 0.25)
        self.assertEqual(result.unit.exponents, {"m": -1})
        self.assertAlmostEqual(result.uncertainty, 0.00625)


class TestQuantityFullCoverage(BaseTestUnit):
    """
    Test suite aiming for 100% coverage of quantity.py by testing
    edge cases and all dunder methods.
    """

    def setUp(self):
        super().setUp()
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        self.system.register_unit("m", length, 1.0, "meter")
        self.system.register_unit("s", time, 1.0, "second")
        self.system.register_unit("kg", mass, 1.0, "kilogram")
        self.system.register_unit("rad", Dimension({}), 1.0, "radian")

    def test_dunder_methods(self):
        """Test various dunder methods."""
        q1 = self.system.Q_(10, "m", uncertainty=0.1)
        q2 = self.system.Q_(5, "m")

        self.assertEqual(self.system.Q_(5, "m") - q2, self.system.Q_(0, "m"))

        self.assertEqual((-q1).magnitude, -10)
        self.assertEqual((+q1).magnitude, 10)
        self.assertEqual(abs(self.system.Q_(-5, "m")).magnitude, 5)

        self.assertEqual(float(q2), 5.0)

        # --- FIX: The test now asserts the CORRECT representation ---
        self.assertEqual(
            repr(q1),
            "Quantity(10, CompoundUnit(exponents={'m': 1}), uncertainty=0.1)",
        )
        self.assertEqual(str(q1), "(10 ± 0.1) m")
        self.assertEqual(str(q2), "5 m")

        q_arr_unc = self.system.Q_(10, "m", uncertainty=np.array([0.1, 0.2]))
        self.assertIn("uncertainty=[...]", str(q_arr_unc))

    def test_comparison_edge_cases(self):
        """Test __le__, __ge__ and comparisons with non-quantities."""
        q1 = self.system.Q_(5, "m")
        q2 = self.system.Q_(5, "m")
        q3 = self.system.Q_(10, "m")

        self.assertLessEqual(q1, q2)
        self.assertLessEqual(q1, q3)
        self.assertGreaterEqual(q2, q1)
        self.assertGreaterEqual(q3, q1)

        self.assertNotEqual(q1, 5)
        with self.assertRaises(TypeError):
            _ = q1 < 5
        with self.assertRaises(TypeError):
            _ = q1 <= 5
        with self.assertRaises(TypeError):
            _ = q1 > 5
        with self.assertRaises(TypeError):
            _ = q1 >= 5

    def test_numpy_ufuncs(self):
        """Test interactions with NumPy universal functions."""
        angle = self.system.Q_(np.pi / 2, "rad", 0.01)
        self.assertAlmostEqual(np.sin(angle).magnitude, 1.0)
        self.assertAlmostEqual(np.cos(angle).magnitude, 0.0)
        self.assertAlmostEqual(
            np.tan(self.system.Q_(np.pi / 4, "rad")).magnitude, 1.0
        )

        with self.assertRaises(IncompatibleUnitsError):
            np.sin(self.system.Q_(1, "m"))

        area = self.system.Q_(16, "m**2")
        side = np.sqrt(area)
        self.assertEqual(side.magnitude, 4.0)
        self.assertEqual(side.unit.exponents, {"m": 1.0})
        self.assertEqual(np.square(side), area)

        arr_q = self.system.Q_(np.array([1, 2, 3]), "m")
        self.assertEqual(np.add.reduce(arr_q), self.system.Q_(6, "m"))

        self.assertEqual(
            np.absolute(self.system.Q_(np.array([-1, -2]), "m")),
            self.system.Q_(np.array([1, 2]), "m"),
        )

    def test_vector_and_array_ops(self):
        """Test dot, cross, len, and getitem."""
        v1 = self.system.Q_(np.array([1, 0, 0]), "m")
        v2 = self.system.Q_(np.array([0, 2, 0]), "m")

        self.assertEqual(v1.dot(v2).magnitude, 0)
        self.assertEqual(v1.dot(v2).unit.exponents, {"m": 2})

        cross_prod = v1.cross(v2)
        np.testing.assert_array_equal(cross_prod.magnitude, [0, 0, 2])
        self.assertEqual(cross_prod.unit.exponents, {"m": 2})

        self.assertEqual(len(v1), 3)
        self.assertEqual(v1[0], self.system.Q_(1, "m"))
        np.testing.assert_array_equal(v1[1:].magnitude, np.array([0, 0]))

        with self.assertRaises(TypeError):
            len(self.system.Q_(1, "m"))
        with self.assertRaises(TypeError):
            _ = (self.system.Q_(1, "m"))[0]

    def test_formatting(self):
        """Test the __format__ method."""
        q = self.system.Q_(1234.567, "m/s**2", 0.02)

        self.assertEqual(format(q, ".2f"), "(1234.57 ± 0.02) m/s²")

        self.system.register_alias({"m": 1, "s": -2}, "acceleration")
        self.assertEqual(format(q, "alias"), "(1234.567 ± 0.02) acceleration")
        self.assertEqual(format(q, ".1f|alias"), "(1234.6 ± 0.0) acceleration")

        q_frac = self.system.Q_(1.5, "m")
        self.assertEqual(format(q_frac, "frac"), "3/2 m")

    def test_latex_representation(self):
        """Test LaTeX output."""
        q_unc = self.system.Q_(10, "m/s", 0.1)
        q_no_unc = self.system.Q_(5, "kg")

        self.assertEqual(q_unc.to_latex(), "(10 \\pm 0.1) \\; \\frac{m}{s}")
        self.assertEqual(q_no_unc.to_latex(), "5 \\; kg")
        self.assertEqual(q_unc._repr_latex_(), f"${q_unc.to_latex()}$")

    def test_multiplication_with_unit(self):
        """Test multiplying a Quantity by a CompoundUnit."""
        q = self.system.Q_(10, "m")
        unit_s = self.system.get_unit("s")
        result = q * unit_s
        self.assertEqual(result.magnitude, 10)
        self.assertEqual(result.unit.exponents, {"m": 1, "s": 1})

    def test_division_by_unit(self):
        """Test dividing a Quantity by a CompoundUnit."""
        q = self.system.Q_(10, "m")
        unit_s = self.system.get_unit("s")
        result = q / unit_s
        self.assertEqual(result.magnitude, 10)
        self.assertEqual(result.unit.exponents, {"m": 1, "s": -1})

    def test_round(self):
        """Test rounding a Quantity."""
        q = self.system.Q_(3.14159, "rad")
        self.assertEqual(round(q, 2).magnitude, 3.14)
        self.assertEqual(round(q).magnitude, 3.0)

    def test_hash(self):
        """Test that Quantity instances are hashable."""
        q1 = self.system.Q_(5, "m")
        q2 = self.system.Q_(5, "m")
        q3 = self.system.Q_(10, "m")

        self.assertEqual(hash(q1), hash(q2))
        self.assertNotEqual(hash(q1), hash(q3))

    def test_edge_case_operations(self):
        """Test edge cases in arithmetic operations."""
        q = self.system.Q_(10, "m")

        self.assertEqual((q * 0).magnitude, 0)
        self.assertEqual((0 * q).magnitude, 0)

        self.assertEqual((q / 1).magnitude, 10)
        self.assertEqual((1 / q).magnitude, 0.1)
        self.assertEqual((1 / q).unit.exponents, {"m": -1})

        zero_q = self.system.Q_(0, "m")
        self.assertEqual((q + zero_q).magnitude, 10)
        self.assertEqual((zero_q + q).magnitude, 10)

        self.assertEqual((q - q).magnitude, 0)

    def test_invalid_operations(self):
        """Test operations that should raise errors."""
        q_length = self.system.Q_(10, "m")
        q_time = self.system.Q_(5, "s")

        with self.assertRaises(IncompatibleUnitsError):
            _ = q_length + q_time
        with self.assertRaises(IncompatibleUnitsError):
            _ = q_length - q_time
        with self.assertRaises(IncompatibleUnitsError):
            _ = q_length < q_time
        with self.assertRaises(TypeError):
            _ = q_length * "invalid"
        with self.assertRaises(TypeError):
            _ = q_length / "invalid"

    def test_division_with_multiple_units(self):
        """Test division resulting in compound units."""
        q1 = self.system.Q_(20, "m")
        q2 = self.system.Q_(4, "s")
        result = q1 / q2
        self.assertEqual(result.magnitude, 5)
        self.assertEqual(result.unit.exponents, {"m": 1, "s": -1})

        q3 = self.system.Q_(4, "m")
        q4 = self.system.Q_(2, "s")
        result = q3 / q4
        self.assertEqual(result.magnitude, 2)
        self.assertEqual(result.unit.exponents, {"m": 1, "s": -1})

    def test_simplification_by_multiplication(self):
        """Test simplification of units through multiplication."""
        q1 = self.system.Q_(10, "m/s")
        q2 = self.system.Q_(2, "s")
        result = q1 * q2
        self.assertEqual(result.magnitude, 20)
        self.assertEqual(result.unit.exponents, {"m": 1})

    def test_simplification_by_division(self):
        """Test simplification of units through division."""
        q1 = self.system.Q_(10, "m")
        q2 = self.system.Q_(2, "m/s")
        result = q1 / q2
        self.assertEqual(result.magnitude, 5)
        self.assertEqual(result.unit.exponents, {"s": 1})

    def test_subtraction_with_uncertainty(self):
        """Test subtraction where one quantity has uncertainty."""
        q1 = self.system.Q_(10.0, "m", uncertainty=0.1)
        q2 = self.system.Q_(3.0, "m")
        result = q1 - q2
        self.assertAlmostEqual(result.magnitude, 7.0)
        self.assertAlmostEqual(result.uncertainty, 0.1)

    def test_subtraction(self):
        """Test subtraction where both quantities have uncertainty."""
        q1 = self.system.Q_(10.0, "m")
        q2 = self.system.Q_(3.0, "m")
        result = q1 - q2
        self.assertAlmostEqual(result.magnitude, 7.0)

    def test__rsub__(self):
        """Test right-side subtraction."""
        q1 = self.system.Q_(10.0, "m")
        q2 = self.system.Q_(3.0, "m")
        result = q2 - q1
        self.assertAlmostEqual(result.magnitude, -7.0)


if __name__ == "__main__":
    unittest.main()
