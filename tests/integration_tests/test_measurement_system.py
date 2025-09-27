# tests/integration_tests/test_measurement_system.py (Refactored)

"""
Integration tests for the MeasureKit measurement system after refactoring.
These tests verify that all components work together using an isolated
UnitSystem instance.
"""

import math
import unittest

from measurekit.exceptions import IncompatibleUnitsError
from tests.base_test_class import BaseTestUnit


class TestMeasurementSystemIntegration(BaseTestUnit):
    """Integration tests for the refactored, system-aware measurement system."""

    def setUp(self):
        """Set up a fully populated, isolated UnitSystem for each test."""
        super().setUp()
        self.add_common_units()

        # Register aliases for compound units
        self.system.register_alias({"m": 1, "s": -1}, "velocity")
        self.system.register_alias({"kg": 1, "m": 1, "s": -2}, "newton")
        self.system.register_alias({"kg": 1, "m": 2, "s": -2}, "joule")

    def test_unit_creation_and_conversion(self):
        """Test creating units and converting between them within a system."""
        meter = self.system.get_unit("m")
        centimeter = self.system.get_unit("cm")
        kilometer = self.system.get_unit("km")

        # Conversion factor methods now require the system context
        self.assertEqual(meter.conversion_factor_to(centimeter), 100.0)
        self.assertEqual(kilometer.conversion_factor_to(meter), 1000.0)

    def test_quantity_creation_and_conversion(self):
        """Test creating quantities and converting them."""
        length1 = self.system.Q_(5.0, "m")
        length2 = self.system.Q_(300.0, "cm")

        # The .to() method implicitly uses the quantity's own system
        length2_m = length2.to("m")
        self.assertEqual(length2_m.magnitude, 3.0)
        self.assertEqual(length2_m.unit, self.system.get_unit("m"))

        # Dimensions are consistent within the same system
        self.assertEqual(length1.dimension, length2.dimension)

    def test_quantity_arithmetic(self):
        """Test arithmetic operations with quantities."""
        length1 = self.system.Q_(5.0, "m")
        length2 = self.system.Q_(300.0, "cm")
        time = self.system.Q_(2.0, "s")

        # Addition handles conversion automatically
        total_length_m = length1 + length2
        self.assertEqual(total_length_m.magnitude, 8.0)
        self.assertEqual(total_length_m.unit, self.system.get_unit("m"))

        # Division creates a new unit
        velocity = length1 / time
        self.assertEqual(velocity.magnitude, 2.5)
        self.assertEqual(velocity.unit.exponents, {"m": 1, "s": -1})

    def test_dimension_consistency(self):
        """Test that dimension consistency is maintained."""
        length = self.system.Q_(5.0, "m")
        time = self.system.Q_(2.0, "s")

        # Different dimensions - should raise error
        with self.assertRaises(IncompatibleUnitsError):
            _ = length + time

    def test_end_to_end_calculation(self):
        """Test a complete physics calculation end-to-end."""
        mass = self.system.Q_(75.0, "kg")
        height = self.system.Q_(10.0, "m")
        g = self.system.Q_(9.81, "m/s^2")

        # E = m*g*h
        potential_energy = mass * g * height
        self.assertAlmostEqual(potential_energy.magnitude, 7357.5)
        self.assertEqual(
            potential_energy.unit.exponents, {"kg": 1, "m": 2, "s": -2}
        )

        # Convert to the aliased "joule" unit
        energy_in_joules = potential_energy.to("J")
        self.assertAlmostEqual(energy_in_joules.magnitude, 7357.5)
        self.assertEqual(energy_in_joules.unit, self.system.get_unit("J"))

        # t = sqrt(2*h/g)
        time_to_fall = (2 * height / g) ** 0.5
        self.assertAlmostEqual(
            time_to_fall.magnitude, math.sqrt(2 * 10 / 9.81)
        )
        self.assertEqual(time_to_fall.unit.exponents, {"s": 1.0})


if __name__ == "__main__":
    unittest.main()
