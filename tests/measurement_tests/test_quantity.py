"""Test suite for the Quantity class."""

import math
import unittest

from measurement.conversions import register_unit
from measurement.dimensions import Dimension
from measurement.quantity import Quantity
from measurement.units import CompoundUnit


from tests.base_test_class import BaseTestUnit

class TestQuantity(BaseTestUnit):
    """Tests for the Quantity class."""

    def setUp(self):
        """Set up common units and dimensions for tests."""
        # Base dimensions
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})

        # Register base units with conversion factors
        register_unit("m", length, 1.0, "meter")
        register_unit("cm", length, 0.01, "centimeter")
        register_unit("km", length, 1000.0, "kilometer")
        register_unit("s", time, 1.0, "second")
        register_unit("min", time, 60.0, "minute")
        register_unit("h", time, 3600.0, "hour")
        register_unit("kg", mass, 1.0, "kilogram")
        register_unit("g", mass, 0.001, "gram")

        # Create common compound units
        self.meter = CompoundUnit({"m": 1})
        self.centimeter = CompoundUnit({"cm": 1})
        self.kilometer = CompoundUnit({"km": 1})
        self.second = CompoundUnit({"s": 1})
        self.minute = CompoundUnit({"min": 1})
        self.kilogram = CompoundUnit({"kg": 1})
        self.gram = CompoundUnit({"g": 1})
        self.newton = CompoundUnit({"kg": 1, "m": 1, "s": -2})

        # Register aliases
        CompoundUnit.register_alias({"m": 1}, "length")
        CompoundUnit.register_alias({"s": 1}, "time")
        CompoundUnit.register_alias({"kg": 1}, "mass")
        CompoundUnit.register_alias({"m": 1, "s": -1}, "velocity", "speed")
        CompoundUnit.register_alias({"kg": 1, "m": 1, "s": -2}, "force")

    def test_initialization(self):
        """Test different initialization patterns."""
        # Basic initialization with value and unit
        q1 = Quantity(5.0, self.meter)
        self.assertEqual(q1.value, 5.0)
        self.assertEqual(q1.unit, self.meter)
        self.assertEqual(q1.dimension, self.meter.dimension)

        # Initialize with another quantity
        q2 = Quantity(q1)
        self.assertEqual(q2.value, 5.0)
        self.assertEqual(q2.unit, self.meter)

        # Initialize with default unit
        QuantityWithMeter = Quantity[self.meter]
        q3 = QuantityWithMeter(10.0)
        self.assertEqual(q3.value, 10.0)
        self.assertEqual(q3.unit, self.meter)

        # Initialize with a different unit but same dimension
        q4 = QuantityWithMeter(10.0, self.centimeter)
        self.assertEqual(q4.value, 0.1)  # Converted to meters (default unit)
        self.assertEqual(q4.unit, self.meter)

        # Initialization without unit should raise error if no default
        with self.assertRaises(ValueError):
            q5 = Quantity(5.0)
            print(q5)

    def test_class_getitem(self):
        """Test the __class_getitem__ method for creating specialized quantity types."""
        # Create a specialized quantity type
        LengthQuantity = Quantity[self.meter]

        # Check the default unit
        self.assertEqual(LengthQuantity.default_unit, self.meter)

        # Create instances
        length1 = LengthQuantity(5.0)
        length2 = LengthQuantity(10.0)

        # Verify correct defaults
        self.assertEqual(length1.unit, self.meter)
        self.assertEqual(length2.unit, self.meter)

        # Caching behavior - getting the same specialized type
        LengthQuantity2 = Quantity[self.meter]
        self.assertIs(LengthQuantity, LengthQuantity2)

    def test_conversion(self):
        """Test unit conversion with the to method."""
        # Create a quantity with meters
        length = Quantity(5.0, self.meter)

        # Convert to centimeters
        length_cm = length.to(self.centimeter)
        self.assertEqual(length_cm.value, 500.0)
        self.assertEqual(length_cm.unit, self.centimeter)

        # Convert to kilometers
        length_km = length.to(self.kilometer)
        self.assertEqual(length_km.value, 0.005)
        self.assertEqual(length_km.unit, self.kilometer)

        # Convert using string unit
        length_cm_str = length.to("cm")
        self.assertEqual(length_cm_str.value, 500.0)
        self.assertEqual(length_cm_str.unit, self.centimeter)

        # Converting between incompatible dimensions should raise error
        time = Quantity(10.0, self.second)
        with self.assertRaises(ValueError):
            time.to(self.meter)

    def test_arithmetic_operations(self):
        """Test arithmetic operations between quantities."""
        # Addition
        length1 = Quantity(5.0, self.meter)
        length2 = Quantity(10.0, self.meter)
        sum_length = length1 + length2
        self.assertEqual(sum_length.value, 15.0)
        self.assertEqual(sum_length.unit, self.meter)

        # Addition with incompatible units should raise error
        time = Quantity(10.0, self.second)
        with self.assertRaises(ValueError):
            length1 + time

        # Subtraction
        diff_length = length2 - length1
        self.assertEqual(diff_length.value, 5.0)
        self.assertEqual(diff_length.unit, self.meter)

        # Multiplication
        time = Quantity(2.0, self.second)
        velocity = length1 / time
        self.assertEqual(velocity.value, 2.5)
        self.assertEqual(velocity.unit.exponents, {"m": 1, "s": -1})

        # Division
        area = Quantity(10.0, self.meter**2)
        length_from_area = area / length1
        self.assertEqual(length_from_area.value, 2.0)
        self.assertEqual(length_from_area.unit, self.meter)

        # Power
        area = length1**2
        self.assertEqual(area.value, 25.0)
        self.assertEqual(area.unit.exponents, {"m": 2})

        # Scalar operations
        double_length = length1 * 2
        self.assertEqual(double_length.value, 10.0)
        self.assertEqual(double_length.unit, self.meter)

        half_length = length1 / 2
        self.assertEqual(half_length.value, 2.5)
        self.assertEqual(half_length.unit, self.meter)

        # Right operations
        double_length_right = 2 * length1
        self.assertEqual(double_length_right.value, 10.0)
        self.assertEqual(double_length_right.unit, self.meter)

        inverse_length = 1 / length1
        self.assertEqual(inverse_length.value, 0.2)
        self.assertEqual(inverse_length.unit.exponents, {"m": -1})

    def test_comparison_operations(self):
        """Test comparison operations between quantities."""
        length1 = Quantity(5.0, self.meter)
        length2 = Quantity(10.0, self.meter)
        length3 = Quantity(5.0, self.meter)

        # Equality
        self.assertEqual(length1, length3)
        self.assertNotEqual(length1, length2)

        # Comparison with incompatible units should raise error
        time = Quantity(5.0, self.second)
        with self.assertRaises(
            ValueError,
            msg="Cannot compare quantities with different dimensions L != T m != s",
        ):
            length1 == time

        # Less than, greater than
        self.assertLess(length1, length2)
        self.assertGreater(length2, length1)
        self.assertLessEqual(length1, length3)
        self.assertGreaterEqual(length1, length3)

    def test_numeric_protocol(self):
        """Test adherence to Python's numeric protocols."""
        length = Quantity(5.0, self.meter)

        # Basic numeric operations
        self.assertEqual(float(length), 5.0)
        self.assertEqual(abs(-length).value, 5.0)
        self.assertEqual((+length).value, 5.0)
        self.assertEqual((-length).value, -5.0)

        # Rounding
        self.assertEqual(round(Quantity(5.6, self.meter)).value, 6.0)
        self.assertEqual(round(Quantity(5.4, self.meter)).value, 5.0)
        self.assertEqual(round(Quantity(5.55, self.meter), 1).value, 5.5)

        # Math functions through Real protocol
        self.assertEqual(math.floor(length), 5)
        self.assertEqual(math.ceil(Quantity(5.1, self.meter)), 6)
        self.assertEqual(int(length), 5)

    def test_formatting(self):
        """Test string formatting methods."""
        length = Quantity(5.0, self.meter)

        # Basic string representation
        self.assertEqual(str(length), "5.0 m")
        self.assertEqual(repr(length), "Quantity(5.0, CompoundUnit({'m': 1}))")

        # Formatting
        self.assertEqual(f"{length:.2f}", "5.00 m")
        self.assertEqual(f"{length:frac}", "5 m")

    def test_extended_arithmetic(self):
        """Test additional arithmetic operations."""
        length1 = Quantity(10.0, self.meter)
        length2 = Quantity(3.0, self.meter)

        # Floor division
        result = length1 // length2
        self.assertEqual(result.value, 3.0)
        self.assertEqual(result.unit.exponents, {})  # dimensionless

        # Modulo
        remainder = length1 % length2
        self.assertEqual(remainder.value, 1.0)
        self.assertEqual(remainder.unit, self.meter)

        # Right floor division
        scalar_div = 20 // length2
        self.assertEqual(scalar_div.value, 6.0)
        self.assertEqual(scalar_div.unit.exponents, {"m": -1})

        # Right modulo
        scalar_mod = 11 % length1
        self.assertEqual(scalar_mod.value, 1.0)
        self.assertEqual(scalar_mod.unit, self.meter)


if __name__ == "__main__":
    unittest.main()
