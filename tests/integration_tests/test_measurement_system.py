"""Integration tests for the MeasureKit measurement system.

These tests verify that the various components of the measurement system work 
together properly, testing end-to-end functionality.
"""
import math
import unittest

from measurement.conversions import (
    UNIT_REGISTRY,
    register_unit,
)
from measurement.dimensions import Dimension
from measurement.quantity import Quantity
from measurement.units import CompoundUnit, get_unit
from tests.base_test_class import BaseTestUnit

# Initial check
print(f"\nInitial UNIT_REGISTRY state: {UNIT_REGISTRY}")

class TestMeasurementSystemIntegration(BaseTestUnit):
    """Integration tests for the measurement system."""

    def setUp(self):
        """Set up common units and dimensions for tests."""
        # Create base dimensions
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        temperature = Dimension({"Θ": 1})
        current = Dimension({"I": 1})
        
        # Derived dimensions
        force = mass * length / (time ** 2)  # F = M·L/T²
        energy = force * length  # E = F·L = M·L²/T²
        
        # Register common units
        # Length units
        register_unit("m", length, 1.0, "meter")
        register_unit("cm", length, 0.01, "centimeter")
        register_unit("km", length, 1000.0, "kilometer")
        register_unit("in", length, 0.0254, "inch")
        register_unit("ft", length, 0.3048, "foot")
        register_unit("mi", length, 1609.344, "mile")
        
        # Time units
        register_unit("s", time, 1.0, "second")
        register_unit("min", time, 60.0, "minute")
        register_unit("h", time, 3600.0, "hour")
        register_unit("d", time, 86400.0, "day")
        
        # Mass units
        register_unit("kg", mass, 1.0, "kilogram")
        register_unit("g", mass, 0.001, "gram")
        register_unit("lb", mass, 0.45359237, "pound")
        
        # Temperature units
        register_unit("K", temperature, 1.0, "kelvin")
        register_unit("°C", temperature, 1.0, "celsius")  # Need conversion function for non-linear
        
        # Current units
        register_unit("A", current, 1.0, "ampere")
        register_unit("mA", current, 0.001, "milliampere")
        
        # Force units
        register_unit("N", force, 1.0, "newton")  # 1 N = 1 kg·m/s²
        
        # Energy units
        register_unit("J", energy, 1.0, "joule")  # 1 J = 1 N·m = 1 kg·m²/s²
        
        # Register aliases for compound units
        CompoundUnit.register_alias({"m": 1, "s": -1}, "velocity", "speed")
        CompoundUnit.register_alias({"m": 2}, "m²", "area")
        CompoundUnit.register_alias({"m": 3}, "m³", "volume")
        CompoundUnit.register_alias({"m": 1, "s": -2}, "m/s²", "acceleration")
        CompoundUnit.register_alias({"kg": 1, "m": 1, "s": -2}, "N", "newton", "force")
        CompoundUnit.register_alias({"kg": 1, "m": 2, "s": -2}, "J", "joule", "energy")
        CompoundUnit.register_alias({"kg": 1, "m": 2, "s": -3}, "W", "watt", "power")
        
        # Verify unit registration
        try:
            self.assertTrue(UNIT_REGISTRY, "No units were registered")
            for dimension in [length, time, mass, temperature, current, force, energy]:
                self.assertIn(dimension, UNIT_REGISTRY, f"Dimension {dimension} not registered")
        except AssertionError as e:
            print(f"\nUnit registration verification failed: {e}")
            raise    

    def test_unit_creation_and_conversion(self):
        """Test creating units and converting between them."""
        # Create unit instances
        meter = get_unit("m")
        centimeter = get_unit("cm")
        kilometer = get_unit("km")
        
        # Test conversion between units
        self.assertEqual(meter.conversion_factor_to(centimeter), 100.0)
        self.assertEqual(centimeter.conversion_factor_to(meter), 0.01)
        self.assertEqual(kilometer.conversion_factor_to(meter), 1000.0)
        self.assertEqual(kilometer.conversion_factor_to(centimeter), 100000.0)
        
        # Test converting a value
        self.assertEqual(meter.convert_value(5.0, centimeter), 500.0)
        self.assertEqual(centimeter.convert_value(200.0, meter), 2.0)
        self.assertEqual(kilometer.convert_value(1.5, meter), 1500.0)

    def test_quantity_creation_and_conversion(self):
        """Test creating quantities and converting between units."""
        # Create some quantities with different units
        length1 = Quantity(5.0, get_unit("m"))
        length2 = Quantity(300.0, get_unit("cm"))
        length3 = Quantity(0.002, get_unit("km"))
        
        # Test conversion between units
        length1_cm = length1.to("cm")
        self.assertEqual(length1_cm.value, 500.0)
        self.assertEqual(length1_cm.unit, get_unit("cm"))
        
        length2_m = length2.to("m")
        self.assertEqual(length2_m.value, 3.0)
        self.assertEqual(length2_m.unit, get_unit("m"))
        
        length3_m = length3.to("m")
        self.assertEqual(length3_m.value, 2.0)
        self.assertEqual(length3_m.unit, get_unit("m"))
        
        # Test that dimensions are preserved
        self.assertEqual(length1.dimension, length2.dimension)
        self.assertEqual(length2.dimension, length3.dimension)

    def test_quantity_arithmetic(self):
        """Test arithmetic operations with quantities."""
        # Create quantities with compatible units
        length1 = Quantity(5.0, get_unit("m"))
        length2 = Quantity(300.0, get_unit("cm"))
        time = Quantity(2.0, get_unit("s"))
        
        # Addition (after conversion)
        total_length_m = length1 + length2.to("m")
        self.assertEqual(total_length_m.value, 8.0)
        self.assertEqual(total_length_m.unit, get_unit("m"))
        
        # Subtraction (after conversion)
        diff_length_m = length1 - length2.to("m")
        self.assertEqual(diff_length_m.value, 2.0)
        self.assertEqual(diff_length_m.unit, get_unit("m"))
        
        # Multiplication creating a new unit
        area = length1 * length1
        self.assertEqual(area.value, 25.0)
        self.assertEqual(area.unit.exponents, {"m": 2})
        
        # Division creating a new unit
        velocity = length1 / time
        self.assertEqual(velocity.value, 2.5)
        self.assertEqual(velocity.unit.exponents, {"m": 1, "s": -1})
        
        # Testing with scalar values
        double_length = length1 * 2
        self.assertEqual(double_length.value, 10.0)
        self.assertEqual(double_length.unit, get_unit("m"))
        
        half_length = length1 / 2
        self.assertEqual(half_length.value, 2.5)
        self.assertEqual(half_length.unit, get_unit("m"))
        
        # Complex calculations
        mass = Quantity(10.0, get_unit("kg"))
        acceleration = Quantity(9.8, get_unit("m/s²"))
        
        force = mass * acceleration
        self.assertEqual(force.value, 98.0)
        self.assertEqual(force.unit.exponents, {"kg": 1, "m": 1, "s": -2})
        self.assertIn("N", force.unit.get_aliases())  # Should have the newton alias

    def test_dimension_consistency(self):
        """Test that dimension consistency is maintained."""
        length = Quantity(5.0, get_unit("m"))
        time = Quantity(2.0, get_unit("s"))
        mass = Quantity(10.0, get_unit("kg"))
        
        # Same dimension, different units - should allow operation
        length_cm = Quantity(200.0, get_unit("cm"))
        self.assertEqual((length + length_cm.to("m")).value, 7.0)
        
        # Different dimensions - should raise error
        with self.assertRaises(ValueError):
            length + time
        
        with self.assertRaises(ValueError):
            length + mass
        
        # Check compound dimensions
        velocity = length / time
        self.assertEqual(velocity.dimension.exponents, {"L": 1, "T": -1})
        
        acceleration = velocity / time
        self.assertEqual(acceleration.dimension.exponents, {"L": 1, "T": -2})
        
        force = mass * acceleration
        self.assertEqual(force.dimension.exponents, {"M": 1, "L": 1, "T": -2})

    def test_scientific_notation_parsing(self):
        """Test parsing of units with scientific notation."""
        # Test parsing unit expressions
        force_unit = get_unit("kg·m·s⁻²")
        self.assertEqual(force_unit.exponents, {"kg": 1, "m": 1, "s": -2})
        self.assertIn("N", force_unit.get_aliases())
        
        # Test direct quantity creation
        energy = Quantity(50.0, get_unit("J"))
        self.assertEqual(energy.unit.exponents, {"kg": 1, "m": 2, "s": -2})
        
        # Test with scientific notation in value
        large_length = Quantity(1.2e6, get_unit("m"))
        self.assertEqual(large_length.value, 1.2e6)
        
        # Test conversion
        large_length_km = large_length.to("km")
        self.assertEqual(large_length_km.value, 1200.0)
        
        small_time = Quantity(1e-9, get_unit("s"))
        self.assertEqual(small_time.value, 1e-9)

    def test_unit_formatting(self):
        """Test string formatting of units."""
        # Test with various format specifiers
        unit = get_unit("m/s")
        self.assertEqual(f"{unit}", "m/s")
        self.assertEqual(f"{unit:alias}", "speed")
        self.assertEqual(f"{unit:full}", "m/s")
        
        # Test formatting with different units
        energy_J = Quantity(1000.0, get_unit("J"))
        self.assertEqual(str(energy_J), "1000.0 m²·kg/s²")
        # Use alias format spec to get "J"
        self.assertEqual(f"{energy_J:alias:J}", "1000.0 J")
        self.assertEqual(f"{energy_J:full}", "1000.0 m²·kg/s²")
        
        # Test formatting with conversion
        energy_kJ = energy_J.to("kg·m²·s⁻²")
        # Now we expect the full unit format by default
        self.assertEqual(str(energy_kJ), "1000.0 m²·kg/s²")
        # But we can still use the alias format spec
        self.assertEqual(f"{energy_kJ:alias}", "1000.0 joule")
        
    def test_end_to_end_calculation(self):
        """Test a complete physics calculation end-to-end."""
        # Set up some initial quantities
        mass = Quantity(75.0, get_unit("kg"))  # Mass of a person
        height = Quantity(10.0, get_unit("m"))  # Height from ground
        g = Quantity(9.81, get_unit("m/s²"))  # Acceleration due to gravity
        
        # Calculate potential energy: E = m*g*h
        potential_energy = mass * g * height
        self.assertAlmostEqual(potential_energy.value, 7357.5)
        self.assertEqual(potential_energy.unit.exponents, {"kg": 1, "m": 2, "s": -2})
        self.assertIn("J", potential_energy.unit.get_aliases())
        
        # Convert to different energy units (if we had them registered)
        # potential_energy_kj = potential_energy.to("kJ")
        
        # Calculate the time to fall (ignoring air resistance): t = sqrt(2*h/g)
        time_to_fall = (2 * height / g) ** 0.5
        self.assertAlmostEqual(time_to_fall.value, math.sqrt(2 * 10 / 9.81))
        self.assertEqual(time_to_fall.unit.exponents, {"s": 1})
        
        # Calculate final velocity: v = g*t
        final_velocity = g * time_to_fall
        self.assertAlmostEqual(final_velocity.value, math.sqrt(2 * 9.81 * 10))
        self.assertEqual(final_velocity.unit.exponents, {"m": 1, "s": -1})
        
        # Calculate kinetic energy at impact: E = 0.5*m*v²
        kinetic_energy = 0.5 * mass * final_velocity ** 2
        self.assertAlmostEqual(kinetic_energy.value, 7357.5)
        self.assertEqual(kinetic_energy.unit.exponents, {"kg": 1, "m": 2, "s": -2})
        
        # Verify conservation of energy
        self.assertAlmostEqual(potential_energy.value, kinetic_energy.value)

if __name__ == '__main__':
    unittest.main()
