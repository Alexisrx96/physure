# tests/measurement_tests/test_units.py (Refactored)

"""Test suite for the CompoundUnit class and get_unit function after refactoring."""

import unittest

from measurekit import get_unit
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.units import CompoundUnit
from tests.base_test_class import BaseTestUnit


class TestCompoundUnit(BaseTestUnit):
    """Tests for the CompoundUnit class using an isolated system."""

    def setUp(self):
        """Set up test fixtures before each test."""
        super().setUp()
        # Aliases are still registered on the class, as they are stateless definitions
        self.system.register_alias({"m": 1, "s": -1}, "velocity", "speed")

        # Register units into our isolated test system
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        self.system.register_unit("m", length, 1.0, "meter")
        self.system.register_unit("s", time, 1.0, "second")
        self.system.register_unit("kg", mass, 1.0, "kilogram")
        self.system.register_unit("cm", length, 0.01, "centimeter")
        self.system.register_unit("km", length, 1000.0, "kilometer")

    def test_init_and_new(self):
        """Test initialization and __new__ caching behavior."""
        unit1 = CompoundUnit({"m": 1})
        self.assertEqual(unit1.exponents, {"m": 1})

        unit2 = CompoundUnit({"m": 1})
        self.assertIs(unit1, unit2)

    def test_arithmetic_operations(self):
        """Test arithmetic operations between units."""
        meter = CompoundUnit({"m": 1})
        second = CompoundUnit({"s": 1})
        kilogram = CompoundUnit({"kg": 1})

        velocity = meter / second
        self.assertEqual(velocity.exponents, {"m": 1, "s": -1})

        area = meter**2
        self.assertEqual(area.exponents, {"m": 2})

        force = kilogram * meter / (second**2)
        self.assertEqual(force.exponents, {"kg": 1, "m": 1, "s": -2})

    def test_dimension(self):
        """Test dimension calculation, which now requires a system."""
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})

        meter = CompoundUnit({"m": 1})
        # The .dimension() method now needs the system to look up definitions
        self.assertEqual(meter.dimension(self.system), length)

        velocity = CompoundUnit({"m": 1, "s": -1})
        self.assertEqual(velocity.dimension(self.system), length / time)

        # Test with an unknown dimension within the context of our system
        with self.assertRaises(ValueError):
            CompoundUnit({"unknown_unit": 1}).dimension(self.system)

    def test_conversion_methods(self):
        """Test methods for unit conversion, which now require a system."""
        meter = CompoundUnit({"m": 1})
        centimeter = CompoundUnit({"cm": 1})
        kilometer = CompoundUnit({"km": 1})

        # Test conversion factor calculation using the system
        self.assertEqual(meter.conversion_factor_to(centimeter), 100.0)
        self.assertEqual(centimeter.conversion_factor_to(meter), 0.01)
        self.assertEqual(kilometer.conversion_factor_to(meter), 1000.0)


class TestGetUnit(BaseTestUnit):
    """
    Tests for the global get_unit function.
    This function should work out-of-the-box as it uses the default_system
    which is configured on library import.
    """

    def test_get_unit_simple(self):
        """Test get_unit with simple expressions."""
        # These units are registered in the default_system upon initialization
        self.assertEqual(get_unit("m").exponents, {"m": 1})
        self.assertEqual(get_unit("kg").exponents, {"kg": 1})
        self.assertEqual(get_unit("m/s").exponents, {"m": 1, "s": -1})

    def test_get_unit_complex(self):
        """Test get_unit with complex expressions."""
        self.assertEqual(
            get_unit("(kg*m)/s^2").exponents, {"kg": 1, "m": 1, "s": -2}
        )


if __name__ == "__main__":
    unittest.main()
