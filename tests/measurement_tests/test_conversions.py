# tests/measurement_tests/test_conversions.py (Refactored)

"""Test suite for the UnitSystem's conversion and registration logic."""

import unittest

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.conversions import UnitDefinition
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.units import CompoundUnit
from tests.base_test_class import BaseTestUnit


class TestUnitDefinition(BaseTestUnit):
    """Tests for the UnitDefinition class (behavior is unchanged)."""

    def test_initialization_and_caching(self):
        """Test initialization and caching behavior."""
        length = Dimension({"L": 1})
        unit1 = UnitDefinition("m", length, LinearConverter(1.0), "meter")
        self.assertEqual(unit1.symbol, "m")
        self.assertEqual(unit1.dimension, length)
        self.assertEqual(unit1.name, "meter")

        unit2 = UnitDefinition("m", length, LinearConverter(1.0), "meter")
        self.assertIs(unit1, unit2)


class TestUnitSystemRegistration(BaseTestUnit):
    """Tests for unit registration and lookup on a UnitSystem instance."""

    def setUp(self):
        """Set up test dimensions."""
        super().setUp()
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})

    def test_register_unit(self):
        """Test registering units in the system's registries."""
        # Register a unit using the system instance
        self.system.register_unit(
            "m", self.length, LinearConverter(1.0), "meter"
        )

        # Check that it's in the system's registries
        self.assertIn("m", self.system.UNIT_REGISTRY[self.length])
        self.assertEqual(self.system.UNIT_DIMENSIONS["m"], self.length)

        # Register another unit of the same dimension
        self.system.register_unit(
            "cm", self.length, LinearConverter(0.01), "centimeter"
        )
        self.assertIn("cm", self.system.UNIT_REGISTRY[self.length])

    def test_find_dimension_for_unit(self):
        """Test finding the dimension for a registered unit."""
        self.system.register_unit(
            "m", self.length, LinearConverter(1.0), "meter"
        )
        self.system.register_unit(
            "s", self.time, LinearConverter(1.0), "second"
        )

        # Dimensions are now stored in the system's UNIT_DIMENSIONS dictionary
        self.assertEqual(self.system.UNIT_DIMENSIONS["m"], self.length)
        self.assertEqual(self.system.UNIT_DIMENSIONS["s"], self.time)

        # Accessing an unregistered unit should raise a KeyError
        with self.assertRaises(KeyError):
            _ = self.system.UNIT_DIMENSIONS["unknown_unit"]


class TestCompoundUnitConversion(BaseTestUnit):
    """Tests for compound unit conversion using the UnitSystem."""

    def setUp(self):
        """Set up test dimensions and units within the system."""
        super().setUp()
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})

        self.system.register_unit(
            "m", self.length, LinearConverter(1.0), "meter"
        )
        self.system.register_unit(
            "cm", self.length, LinearConverter(0.01), "centimeter"
        )
        self.system.register_unit(
            "km", self.length, LinearConverter(1000.0), "kilometer"
        )
        self.system.register_unit(
            "s", self.time, LinearConverter(1.0), "second"
        )
        self.system.register_unit(
            "min", self.time, LinearConverter(60.0), "minute"
        )
        self.system.register_unit(
            "h", self.time, LinearConverter(3600.0), "hour"
        )
        self.system.register_unit(
            "kg", self.mass, LinearConverter(1.0), "kilogram"
        )
        self.system.register_unit(
            "g", self.mass, LinearConverter(0.001), "gram"
        )

    def test_compound_unit_conversion_factor(self):
        """Test getting conversion factors between compound units."""
        meter = CompoundUnit({"m": 1})
        centimeter = CompoundUnit({"cm": 1})
        self.assertEqual(meter.conversion_factor_to(centimeter), 100.0)

        velocity_mps = CompoundUnit({"m": 1, "s": -1})
        velocity_kmph = CompoundUnit({"km": 1, "h": -1})
        self.assertAlmostEqual(
            velocity_mps.conversion_factor_to(velocity_kmph), 3.6
        )

        force_newton = CompoundUnit({"kg": 1, "m": 1, "s": -2})
        force_dyne = CompoundUnit({"g": 1, "cm": 1, "s": -2})
        self.assertAlmostEqual(
            force_newton.conversion_factor_to(force_dyne), 1e5
        )

        # Test incompatible dimensions
        length = CompoundUnit({"m": 1})
        time = CompoundUnit({"s": 1})
        with self.assertRaises(IncompatibleUnitsError):
            length.conversion_factor_to(time)


if __name__ == "__main__":
    unittest.main()
