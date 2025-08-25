"""Test suite for the CompoundUnit class and get_unit function."""
import unittest

from measurement.conversions import register_unit
from measurement.dimensions import Dimension
from measurement.units import CompoundUnit, get_unit
from measurement.conversions import UNIT_REGISTRY, UNIT_DIMENSIONS

from tests.base_test_class import BaseTestUnit

class TestCompoundUnit(BaseTestUnit):
    """Tests for the CompoundUnit class."""

    def setUp(self):
        """Set up test fixtures before each test."""
        # Register velocity aliases for m/s
        CompoundUnit.register_alias({"m": 1, "s": -1},"speed", "velocity",)


    def test_init_and_new(self):
        """Test initialization and __new__ caching behavior."""
        # Test basic initialization
        unit1 = CompoundUnit({"m": 1})
        self.assertEqual(unit1.exponents, {"m": 1})
        
        # Test caching through __new__ (same exponents should return same instance)
        unit2 = CompoundUnit({"m": 1})
        self.assertIs(unit1, unit2)  # Same instance, not just equality
        
        # Different exponents should create different instances
        unit3 = CompoundUnit({"m": 2})
        self.assertIsNot(unit1, unit3)
        
        # Zero exponents should be removed
        unit4 = CompoundUnit({"m": 1, "kg": 0})
        self.assertEqual(unit4.exponents, {"m": 1})
        self.assertIs(unit4, unit1)  # Same as first unit after normalization

    def test_register_alias(self):
        """Test alias registration and retrieval."""
        unit = CompoundUnit({"m": 1, "s": -1})
        

        # Aliases are already registered in setUp
        self.assertIn("velocity", unit.get_aliases())
        self.assertIn("speed", unit.get_aliases())
        
        # Check the to_string method with alias
        self.assertEqual(unit.to_string(use_alias=True), "velocity")
        
        # Check with preference
        self.assertEqual(unit.to_string(use_alias=True, alias_preference="velocity"), "velocity")
        self.assertEqual(unit.to_string(use_alias=True, alias_preference="speed"), "speed")
        
        # Check the to_string method without alias
        self.assertEqual(unit.to_string(use_alias=False), "m/s")
        
        # Check that format spec with alias works correctly
        self.assertEqual(f"{unit:alias}", "velocity")

    def test_to_string(self):
        """Test string representation in various formats."""
        # Simple unit
        unit1 = CompoundUnit({"m": 1})
        self.assertEqual(unit1.to_string(False), "m")
        
        # Unit with exponent
        unit2 = CompoundUnit({"m": 2})
        self.assertEqual(unit2.to_string(False), "m²")
        
        # Multiple units in numerator
        unit3 = CompoundUnit({"m": 1, "kg": 1})
        self.assertEqual(unit3.to_string(False), "kg·m")  # Should be sorted
        
        # Unit with denominator
        unit4 = CompoundUnit({"m": 1, "s": -1})
        self.assertEqual(unit4.to_string(False), "m/s")
        
        # Complex unit with numerator and denominator
        unit5 = CompoundUnit({"m": 1, "kg": 1, "s": -2})
        self.assertEqual(unit5.to_string(False), "kg·m/s²")
        
        # Only denominators
        unit6 = CompoundUnit({"s": -1})
        self.assertEqual(unit6.to_string(False), "1/s")
        
        # Unit with no exponents
        unit7 = CompoundUnit({})
        self.assertEqual(unit7.to_string(False), "1")

    def test_format(self):
        """Test the __format__ method."""
        unit = CompoundUnit({"m": 1, "s": -1})
        
        # Default format should use alias-free representation
        self.assertEqual(f"{unit}", "m/s")
        
        # 'alias' format spec should use alias
        self.assertEqual(f"{unit:alias}", "velocity")
        
        # 'full' format spec should not use alias
        self.assertEqual(f"{unit:full}", "m/s")
        
        # 'alias:X' format spec should use specific alias if available
        self.assertEqual(f"{unit:alias:speed}", "speed")
        self.assertEqual(f"{unit:alias:velocity}", "velocity")

    def test_str_and_repr(self):
        """Test the __str__ and __repr__ methods."""
        unit = CompoundUnit({"m": 1, "s": -1})
        
        # __str__ should use alias-free representation
        self.assertEqual(str(unit), "m/s")
        
        # __repr__ should show the internal structure
        self.assertEqual(repr(unit), "CompoundUnit({'m': 1, 's': -1})")

    def test_equality_and_hash(self):
        """Test equality comparison and hashing."""
        unit1 = CompoundUnit({"m": 1, "s": -1})
        unit2 = CompoundUnit({"m": 1, "s": -1})
        unit3 = CompoundUnit({"kg": 1})
        
        # Same exponents should be equal
        self.assertEqual(unit1, unit2)
        self.assertEqual(hash(unit1), hash(unit2))
        
        # Different exponents should not be equal
        self.assertNotEqual(unit1, unit3)
        self.assertNotEqual(hash(unit1), hash(unit3))
        
        # Should not be equal to non-CompoundUnit objects
        self.assertNotEqual(unit1, "not a unit")

    def test_arithmetic_operations(self):
        """Test arithmetic operations between units."""
        meter = CompoundUnit({"m": 1})
        second = CompoundUnit({"s": 1})
        kilogram = CompoundUnit({"kg": 1})
        
        # Multiplication
        velocity = meter / second
        self.assertEqual(velocity.exponents, {"m": 1, "s": -1})
        
        # Division
        frequency = 1 / second
        self.assertEqual(frequency.exponents, {"s": -1})
        
        # Power
        area = meter ** 2
        self.assertEqual(area.exponents, {"m": 2})
        
        # Complex arithmetic
        force = kilogram * meter / (second ** 2)
        self.assertEqual(force.exponents, {"kg": 1, "m": 1, "s": -2})

    def test_dimension(self):
        """Test dimension calculation."""
        # Set up some dimensions and register them
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        
        # Register units with their dimensions
        register_unit("m", length, 1.0, "meter")
        register_unit("s", time, 1.0, "second")
        register_unit("kg", mass, 1.0, "kilogram")
        
        # Test dimension property
        meter = CompoundUnit({"m": 1})
        self.assertEqual(meter.dimension, length)
        
        second = CompoundUnit({"s": 1})
        self.assertEqual(second.dimension, time)
        
        # Test compound dimensions
        velocity = CompoundUnit({"m": 1, "s": -1})
        self.assertEqual(velocity.dimension, length / time)
        
        # Test with unknown dimension
        with self.assertRaises(ValueError, msg="Unknown dimension for unit"):
            CompoundUnit({"unknown_unit": 1}).dimension

    def test_conversion_methods(self):
        """Test methods for unit conversion."""
        # Register units with conversion factors
        length = Dimension({"L": 1})
        register_unit("m", length, 1.0, "meter")
        register_unit("cm", length, 0.01, "centimeter")
        register_unit("km", length, 1000.0, "kilometer")
        
        meter = CompoundUnit({"m": 1})
        centimeter = CompoundUnit({"cm": 1})
        kilometer = CompoundUnit({"km": 1})
        
        # Test conversion factor calculation
        self.assertEqual(meter.conversion_factor_to(centimeter), 100.0)
        self.assertEqual(centimeter.conversion_factor_to(meter), 0.01)
        self.assertEqual(kilometer.conversion_factor_to(meter), 1000.0)
        
        # Test value conversion
        self.assertEqual(meter.convert_value(1.0, centimeter), 100.0)
        self.assertEqual(centimeter.convert_value(100.0, meter), 1.0)
        self.assertEqual(kilometer.convert_value(1.0, centimeter), 100000.0)


class TestGetUnit(unittest.TestCase):
    """Tests for the get_unit function."""

    def test_get_unit_simple(self):
        """Test get_unit with simple expressions."""
        # Basic units
        self.assertEqual(get_unit("m").exponents, {"m": 1})
        self.assertEqual(get_unit("kg").exponents, {"kg": 1})
        self.assertEqual(get_unit("s").exponents, {"s": 1})
        
        # Multiple units
        self.assertEqual(get_unit("m·s").exponents, {"m": 1, "s": 1})
        self.assertEqual(get_unit("m/s").exponents, {"m": 1, "s": -1})
        
        # With exponents
        self.assertEqual(get_unit("m²").exponents, {"m": 2})
        self.assertEqual(get_unit("m^2").exponents, {"m": 2})
        self.assertEqual(get_unit("m⁻¹").exponents, {"m": -1})
        self.assertEqual(get_unit("m^-1").exponents, {"m": -1})

    def test_get_unit_complex(self):
        """Test get_unit with complex expressions."""
        # Parentheses
        self.assertEqual(get_unit("(m/s)").exponents, {"m": 1, "s": -1})
        self.assertEqual(get_unit("(m/s)²").exponents, {"m": 2, "s": -2})
        
        # Mixed notations
        self.assertEqual(get_unit("kg·m·s⁻²").exponents, {"kg": 1, "m": 1, "s": -2})
        self.assertEqual(get_unit("kg*m/s^2").exponents, {"kg": 1, "m": 1, "s": -2})
        
        # Complex expressions
        self.assertEqual(get_unit("(kg·m²)/(s²·A)").exponents, {"kg": 1, "m": 2, "s": -2, "A": -1})

if __name__ == '__main__':
    unittest.main()
